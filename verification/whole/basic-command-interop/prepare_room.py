#!/usr/bin/env python3
"""Observe room placement, Welcome delivery and upstream admission on one owned relay."""
import argparse
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import signal
import socket
import struct
import subprocess
import tempfile

from observe import (BRIDGE, RELAY, account_options, copy_verified, file_digest,
                     host_preferences_state, json_command, until, verify_profile)
from isolate import Workspace
from composed_preflight import pinned_json
from provider_activation import phase, publication, read_exact

UNTRUSTED_ACCOUNT = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
RELAY_PIN = "7c5cbfee478f225bdfe8d5b83bf17612772adf8ead13e36d7d47a02ba24e14e3"


def exchange(path, request):
    payload = json.dumps(request, separators=(",", ":")).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(3)
        stream.connect(str(path))
        stream.sendall(struct.pack(">I", len(payload)) + payload)
        stream.shutdown(socket.SHUT_WR)
        size = struct.unpack(">I", read_exact(stream, 4))[0]
        if not 0 < size <= 65536:
            raise ValueError("invalid prepare reply length")
        body = json.loads(read_exact(stream, size))
        if stream.recv(1):
            raise ValueError("unexpected response tail")
        return body


def select_rows(instance, role, manifest, kind):
    root = instance.root / "state" / role
    secret = (root / "keys" / f"sqlcipher-{manifest['sqlcipher_ref']}").read_bytes()
    if len(secret) != 32:
        raise ValueError("unexpected disposable journal key length")
    prk = hmac.digest(b"marmish/v2/storage", secret, "sha256")
    key = hmac.digest(prk, b"journal\x01", "sha256").hex()
    result = instance.run([instance.root / "inspect-journal", kind, root / "journal.db"],
                          input_bytes=f"x'{key}'".encode(), timeout=5)
    if result.returncode or result.stderr:
        raise ValueError(f"could not inspect disposable {kind} journal")
    return [tuple(row.split()) for row in result.stdout.decode("ascii").splitlines()]


def observe(args):
    before = host_preferences_state()
    pins = {"client": {"marmish": args.cli_sha, "marmishd": args.daemon_sha},
            "server": json.loads(BRIDGE.read_bytes())["builds"]["default"]["sha256"]}
    build = pinned_json(args.generated / "receipt.json", args.build_receipt_sha256)
    if {name: record["sha256"] for name, record in build["artifacts"].items()} != pins["client"]:
        raise ValueError("cold build receipt does not bind both candidate executables")
    mounts = {"client": args.generated, "server": args.reference}
    workspace = Workspace()
    with workspace:
        peers = {role: workspace.instance(role) for role in mounts}
        service = workspace.instance("relay")
        for role, instance in peers.items():
            programs = []
            for name, digest in pins[role].items():
                target = instance.root / name
                copy_verified(mounts[role] / name, target, digest)
                programs.append(target)
            helper = instance.root / "inspect-journal"
            copy_verified(args.journal_helper, helper, args.journal_sha)
            instance.allow_executables([*programs, helper])
        relay = service.root / "deaddrop"
        copy_verified(args.relay, relay, RELAY_PIN)
        service.allow_executables([relay])
        base_options = {}
        manifests = {}
        reports = {}
        sockets = {}
        with tempfile.TemporaryFile(dir=service.root) as transcript:
            relay_dir = service.root / "data"
            process = service.start([relay, "relay", "--data-dir", relay_dir],
                                    stdin=subprocess.DEVNULL, stdout=transcript, stderr=subprocess.DEVNULL)

            def endpoint():
                if process.poll() is not None:
                    raise ValueError("owned relay stopped during startup")
                line = os.pread(transcript.fileno(), 1024, 0)
                if not line.endswith(b"\n"):
                    return None
                value = json.loads(line)["relay_url"]
                if not RELAY.fullmatch(value):
                    raise ValueError("unexpected owned relay URL")
                return value

            address = until(endpoint, 150, "relay")
            for role, instance in peers.items():
                options = account_options(instance, role, address)
                base_options[role] = options
                reports[role] = json_command(instance, [instance.root / "marmish", *options, "setup",
                                                        "--role", role, "--no-grants"])
                manifests[role] = verify_profile(instance, role)
                sockets[role] = instance.tmp / f"mr2-{os.geteuid()}-{manifests[role]['profile_id']}" / "s"
            for role, instance in peers.items():
                other = "server" if role == "client" else "client"
                option = "--trust" if role == "client" else "--grant"
                json_command(instance, [instance.root / "marmish", *base_options[role], "setup",
                                        "--update", option, reports[other]["npub"]])
            client = manifests["client"]["public_key"]
            server = manifests["server"]["public_key"]
            if client == server:
                raise ValueError("shared disposable account")
            for role, instance in peers.items():
                config = json.loads((instance.root / "state" / role / "config.json").read_bytes())
                expected = ("trusted_remotes", server) if role == "client" else ("execution_clients", client)
                if config["role"] != role or config["relays"] != [address] or config[expected[0]] != [expected[1]]:
                    raise ValueError("setup did not bind the intended role, peer and relay")

            def start(role):
                instance = peers[role]
                argv = [instance.root / "marmishd", *base_options[role], "--foreground"]
                if role == "server":
                    argv.append("daemon")
                return instance.start(argv, stdin=subprocess.DEVNULL,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)

            owners = {"client": start("client")}

            class UpstreamActivationDeadline(Exception):
                pass

            def wait_owner(role):
                child = owners[role]
                if child.poll() is not None:
                    diagnostic = child.stderr.read(512)
                    if role == "server" and (child.returncode, diagnostic) == (
                        1, b"marmish: operation deadline exceeded\n"
                    ):
                        raise UpstreamActivationDeadline
                    raise ValueError(f"{role} owner exited with {child.returncode}: {diagnostic!r}")
                if role == "client":
                    return phase(sockets[role]) == "provider_active"
                instance = peers[role]
                result = instance.run([instance.root / "marmish", *base_options[role], "status"], timeout=5)
                return result.returncode == 0 and json.loads(result.stdout)["result"]["state"] == "active"

            until(lambda: wait_owner("client"), 190, "client")
            publication(relay_dir / "relay.sqlite3", client)

            request = {"operation": "command_prepare", "remote": server, "relays": [address]}
            for altered, category in ((dict(request, remote=client), "command_unauthorized"),
                                      (dict(request, remote=UNTRUSTED_ACCOUNT), "command_unauthorized"),
                                      (dict(request, relays=[]), "command_relay_binding")):
                response = exchange(sockets["client"], altered)
                if response != {"ok": True, "result": None, "error": None,
                                "command": {"state": "failure", "category": category, "handle": None}}:
                    raise ValueError(f"unexpected refusal for {category}: {response!r}")
            if select_rows(peers["client"], "client", manifests["client"], "placement"):
                raise ValueError("rejected prepare allocated a room")

            busy = {"ok": True, "result": None, "error": None, "command": {"state": "local_busy"}}

            def missing_peer():
                reply = exchange(sockets["client"], request)
                if reply == busy:
                    return None
                if reply != {"ok": True, "result": None, "error": None,
                             "command": {"state": "failure", "category": "key_package_unavailable", "handle": None}}:
                    raise ValueError("unpublished peer did not produce a typed unavailable reply")
                return True

            until(missing_peer, 16, "missing key package")
            if select_rows(peers["client"], "client", manifests["client"], "placement"):
                raise ValueError("failed discovery allocated a room")
            for upstream_starts in (1, 2):
                owners["server"] = start("server")
                try:
                    until(lambda: wait_owner("server"), 190, "server")
                except UpstreamActivationDeadline:
                    if upstream_starts == 2:
                        raise
                    failed = owners["server"]
                    peers["server"].check_stopped(failed)
                    peers["server"].processes.remove(failed)
                    if sockets["server"].exists():
                        raise ValueError("upstream timeout retained its runtime socket")
                else:
                    break
            publication(relay_dir / "relay.sqlite3", server)

            def ready():
                reply = exchange(sockets["client"], request)
                if reply == busy:
                    return None
                command = reply.get("command") or {}
                session = command.get("session", "")
                if not re.fullmatch(r"[0-9a-f]{32}", session) or reply != {
                    "ok": True, "result": None, "error": None,
                    "command": {"state": "ready", "local_account": client, "session": session},
                }:
                    raise ValueError(f"unexpected prepare reply state: {command.get('category', reply.get('error'))!r}")
                return session

            session = until(ready, 20, "prepare")
            rows = select_rows(peers["client"], "client", manifests["client"], "placement")
            if len(rows) != 1:
                raise ValueError("prepare did not commit exactly one room")
            local, remote, bound_relay, bound_session, group, state = rows[0]
            if (local, remote, bound_relay, bound_session, state) != (client, server, address, session, "1") or not group:
                raise ValueError("local durable room does not match the prepared fixture")

            def admitted():
                rows = select_rows(peers["server"], "server", manifests["server"], "admission")
                if rows and rows != [(session.upper(), client.upper(), group, "1", "0")]:
                    raise ValueError("upstream retained a room with the wrong binding")
                return rows

            until(admitted, 30, "upstream admission")
            if ready() != session:
                raise ValueError("duplicate prepare changed the durable session")
            owner = owners["client"]
            owner.send_signal(signal.SIGTERM)
            output, errors = owner.communicate(timeout=20)
            peers["client"].check_stopped(owner)
            peers["client"].processes.remove(owner)
            if owner.returncode or errors or sockets["client"].exists():
                raise ValueError(f"client owner failed its first stop (exit={owner.returncode}, "
                                 f"output={output[:512]!r}, errors={errors[:512]!r}, "
                                 f"socket_exists={sockets['client'].exists()})")
            select_rows(peers["client"], "client", manifests["client"], "ack-boundary")
            if select_rows(peers["client"], "client", manifests["client"], "placement") != [(*rows[0][:-1], "0")]:
                raise ValueError("fixture did not reproduce the persisted acknowledgement boundary")
            owners["client"] = start("client")
            until(lambda: wait_owner("client"), 190, "reopened client")
            if until(ready, 20, "reopened preparation") != session or not admitted():
                raise ValueError("restored client did not reuse the admitted room")
    if workspace.root.exists() or host_preferences_state() != before:
        raise ValueError("workspace or host isolation changed")
    return {"schema": 1, "referencePins": pins["server"], "generatedPins": pins["client"],
            "buildReceiptSha256": args.build_receipt_sha256, "upstreamStarts": upstream_starts,
            "relaySha256": RELAY_PIN, "journalHelperSha256": args.journal_sha,
            "fixtureSha256": file_digest(Path(__file__)), "durablePlacement": True,
            "upstreamAdmissionMatched": True, "wrongPeerAndRouteRejected": True,
            "discoveryRecoversWhenPeerAppears": True,
            "sameSessionAfterDuplicateAndRestart": True,
            "recoversAfterSimulatedReadyCommitLoss": True, "cleanup": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ("generated", "reference", "relay", "journal_helper", "receipt"):
        parser.add_argument(field, type=lambda path: Path(path).resolve())
    parser.add_argument("--cli-sha", required=True)
    parser.add_argument("--daemon-sha", required=True)
    parser.add_argument("--journal-sha", required=True)
    parser.add_argument("--build-receipt-sha256", required=True)
    arguments = parser.parse_args()
    result = observe(arguments)
    with arguments.receipt.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
