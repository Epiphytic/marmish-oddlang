#!/usr/bin/env python3
"""Replay generated profile setup and relay activation in one candidate."""
import argparse
import json
import os
from pathlib import Path
import runpy
import signal
import subprocess
import tempfile

from provider_activation import RELAY_URL, digest, phase, publication, stage, until


def setup(peer, command, *args):
    result = peer.run([*command, "setup", *args], timeout=30)
    if result.returncode or result.stderr:
        raise ValueError("generated file-backed setup failed")
    return json.loads(result.stdout)


def configured_profile(peer, identity):
    root = peer.root / "state" / identity
    manifest = json.loads((root / "profile.json").read_bytes())
    config = root / "config.json"
    if manifest["phase"] != "ready" or manifest["backend"] != "files":
        raise ValueError("setup did not commit a ready file profile")
    if config.stat().st_mode & 0o777 != 0o600:
        raise ValueError("configuration is not private")
    return manifest, config


def observe(args):
    build_receipt = args.export / "receipt.json"
    if digest(build_receipt) != args.build_receipt_sha256:
        raise ValueError("candidate build receipt digest differs")
    build = json.loads(build_receipt.read_bytes())
    if "workspace-composed-basic-owner.json" not in build["stagedContracts"]:
        raise ValueError("incorrect candidate build descriptor")
    workspace = runpy.run_path(str(args.isolation))["Workspace"]()
    with workspace as cohort:
        client = cohort.instance("client")
        server = cohort.instance("server")
        relay = cohort.instance("relay")
        commands = {}
        for role, peer in (("client", client), ("server", server)):
            program = stage(args.export / "marmish", peer.root / "marmish",
                            build["artifacts"]["marmish"]["sha256"])
            commands[role] = [program, "--state-dir", peer.root / "state",
                              "--identity", role, "--key-storage", "file"]
            peer.allow_executables([program])
        daemon = stage(args.export / "marmishd", server.root / "marmishd",
                       build["artifacts"]["marmishd"]["sha256"])
        server.allow_executables([commands["server"][0], daemon])
        service = stage(args.relay, relay.root / "deaddrop", args.relay_sha256)
        relay.allow_executables([service])
        with tempfile.TemporaryFile(dir=relay.root) as transcript:
            data = relay.root / "data"
            process = relay.start([service, "relay", "--data-dir", data],
                                  stdin=subprocess.DEVNULL, stdout=transcript, stderr=subprocess.DEVNULL)

            def address():
                if process.poll() is not None:
                    raise ValueError("test relay stopped before publishing its endpoint")
                row = os.pread(transcript.fileno(), 1024, 0)
                if not row.endswith(b"\n"):
                    return None
                endpoint = json.loads(row)["relay_url"]
                if not RELAY_URL.fullmatch(endpoint):
                    raise ValueError("test relay returned an unexpected endpoint")
                return endpoint

            endpoint = until(address, 180)
            for command in commands.values():
                command.extend(["--tor", "auto", "--relay", endpoint])
            client_report = setup(client, commands["client"], "--role", "client", "--no-grants")
            server_report = setup(server, commands["server"], "--role", "server", "--no-grants")
            if client_report["npub"] == server_report["npub"]:
                raise ValueError("profiles share a signing identity")
            setup(client, commands["client"], "--update", "--trust", server_report["npub"])
            setup(server, commands["server"], "--update", "--grant", client_report["npub"])
            client_manifest, client_config = configured_profile(client, "client")
            server_manifest, server_config = configured_profile(server, "server")
            snapshots = [client_config.read_bytes(), server_config.read_bytes()]
            for role, payload, grants, trusted in (
                ("client", snapshots[0], [], [server_manifest["public_key"]]),
                ("server", snapshots[1], [client_manifest["public_key"]], []),
            ):
                config = json.loads(payload)
                if (config["role"] != role or config["execution_clients"] != grants
                        or config["trusted_remotes"] != trusted or config["relays"] != [endpoint]
                        or config["tor"] != "auto"):
                    raise ValueError("generated authorization configuration differs")
            runtime = server.tmp / f"mr2-{os.geteuid()}-{server_manifest['profile_id']}"
            owner = server.start(
                [daemon, "--foreground", *commands["server"][1:]],
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )

            def activated():
                if owner.poll() is not None:
                    _, error = owner.communicate()
                    raise ValueError(f"composed provider failed to activate: {error.decode().strip()}")
                return phase(runtime / "s") == "provider_active"

            until(activated, 315)
            publication(data / "relay.sqlite3", server_manifest["public_key"])
            owner.send_signal(signal.SIGTERM)
            stdout, stderr = owner.communicate(timeout=30)
            server.check_stopped(owner)
            server.processes.remove(owner)
            report = {"schema": 1, "engine_quiesced": False, "engine_stop_complete": True,
                      "mdk_stop_complete": True, "runtime_cleanup_complete": True}
            expected = (json.dumps(report, separators=(",", ":")) + "\n").encode()
            if owner.returncode or stderr or stdout != expected or runtime.exists():
                raise ValueError("composed provider shutdown differed")
            if snapshots != [client_config.read_bytes(), server_config.read_bytes()]:
                raise ValueError("provider startup changed prepared authorization configuration")
    if workspace.root.exists():
        raise ValueError("isolated test workspace survived")
    return {
        "schema": 1,
        "buildReceiptSha256": digest(args.export / "receipt.json"),
        "isolationFixtureSha256": digest(args.isolation),
        "providerFixtureSha256": digest(Path(__file__)),
        "relaySha256": args.relay_sha256,
        "distinctGeneratedFileProfiles": True,
        "matchingAuthorizedPeerRoles": True,
        "providerActiveAfterAcknowledgement": True,
        "retainedAuthorizationUnchanged": True,
        "ownedCleanup": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("export", "relay", "isolation", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    parser.add_argument("--relay-sha256", required=True)
    parser.add_argument("--build-receipt-sha256", required=True)
    args = parser.parse_args()
    args.receipt.write_text(json.dumps(observe(args), indent=2) + "\n")


if __name__ == "__main__":
    main()
