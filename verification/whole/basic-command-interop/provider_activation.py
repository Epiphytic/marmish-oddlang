#!/usr/bin/env python3
"""Verify the generated foreground provider against an independently owned relay."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import signal
import socket
import sqlite3
import struct
import subprocess
import tempfile
import time

RELAY_URL = re.compile(r"ws://[a-z2-7]{56}\.onion/relay")


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def stage(source, destination, expected):
    shutil.copyfile(source, destination)
    destination.chmod(0o500)
    if digest(destination) != expected:
        raise ValueError("participant digest mismatch")
    return destination


def until(probe, limit):
    deadline = time.monotonic() + limit
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(0.25)
    raise TimeoutError("owned process did not reach the expected phase")


def read_exact(stream, size):
    data = bytearray()
    while len(data) < size:
        chunk = stream.recv(size - len(data))
        if not chunk:
            raise ValueError("incomplete status response")
        data.extend(chunk)
    return bytes(data)


def phase(path):
    if not path.exists():
        return None
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(10)
        stream.connect(str(path))
        request = b'{"operation":"status"}'
        stream.sendall(struct.pack(">I", len(request)) + request)
        stream.shutdown(socket.SHUT_WR)
        size = struct.unpack(">I", read_exact(stream, 4))[0]
        if size > 65536:
            raise ValueError("oversized status response")
        reply = json.loads(read_exact(stream, size))
        if reply["ok"]:
            return reply["result"]["state"]
        raise ValueError("status holder refused the declared operation")


def publication(data, identity):
    with sqlite3.connect(data.as_uri() + "?mode=ro", uri=True) as db:
        rows = db.execute(
            "SELECT id, d_tag, event_json FROM events WHERE pubkey = ? AND kind = 30443",
            (identity,),
        ).fetchall()
    if len(rows) != 1:
        raise ValueError("relay does not hold one published key package for this identity")
    event_id, slot, text = rows[0]
    event = json.loads(text)
    if (event["id"] != event_id or event["pubkey"] != identity or event["kind"] != 30443
            or ["d", slot] not in event["tags"] or not re.fullmatch("[0-9a-f]{128}", event["sig"])):
        raise ValueError("relay key package does not bind the authored identity, slot and signature")
    return event_id, slot, text


def observe(args):
    workspace = runpy.run_path(str(args.isolation))["Workspace"]()
    with workspace as root:
        owner = root.instance("provider")
        relay = root.instance("relay")
        client = stage(args.client, owner.root / "marmish", args.client_sha256)
        daemon = stage(args.daemon, owner.root / "marmishd", args.daemon_sha256)
        service = stage(args.relay, relay.root / "relay", args.relay_sha256)
        state = owner.root / "state"
        options = ["--state-dir", state, "--identity", "provider", "--key-storage", "file"]
        initialized = owner.run(
            [client, *options, "--tor", "off", "--relay", "wss://127.0.0.1:9/", "init"],
            timeout=30,
        )
        if initialized.returncode or initialized.stderr:
            raise ValueError("generated private profile initialization failed")
        invalid = owner.run([daemon, "--foreground", *options, "--tor", "auto",
                             "--relay", "ws://127.0.0.1:9/relay"], timeout=10)
        if invalid.returncode != 125 or invalid.stdout:
            raise ValueError("non-onion WebSocket escaped relay admission")
        profile = json.loads((state / "provider/profile.json").read_bytes())
        socket_dir = owner.tmp / f"mr2-{os.geteuid()}-{profile['profile_id']}"
        with tempfile.TemporaryFile(dir=relay.root) as transcript:
            relay_data = relay.root / "data"
            relay_process = relay.start(
                [service, "relay", "--data-dir", relay_data],
                stdin=subprocess.DEVNULL, stdout=transcript, stderr=subprocess.DEVNULL,
            )

            def address():
                if relay_process.poll() is not None:
                    raise ValueError("test relay exited")
                first = os.pread(transcript.fileno(), 1024, 0)
                if not first.endswith(b"\n"):
                    return None
                endpoint = json.loads(first)["relay_url"]
                if not RELAY_URL.fullmatch(endpoint):
                    raise ValueError("unexpected test relay address")
                return endpoint

            endpoint = until(address, 180)
            records = []
            for _ in range(2):
                server = owner.start(
                    [daemon, "--foreground", *options, "--tor", "auto", "--relay", endpoint],
                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )

                def ready():
                    if server.poll() is not None:
                        _, error = server.communicate()
                        raise ValueError(f"owner exited before provider activation: {error.decode().strip()}")
                    return phase(socket_dir / "s") == "provider_active"

                until(ready, 315)
                records.append(publication(relay_data / "relay.sqlite3", profile["public_key"]))
                server.send_signal(signal.SIGTERM)
                stdout, stderr = server.communicate(timeout=20)
                owner.check_stopped(server)
                owner.processes.remove(server)
                report = json.loads(stdout)
                if (server.returncode or stderr or not report["mdk_stop_complete"]
                        or not report["runtime_cleanup_complete"] or socket_dir.exists()):
                    raise ValueError("generated provider did not shut down and release its private socket")
            if records[0] != records[1]:
                raise ValueError("restart changed the stable signed key package")
    return {
        "schema": 1,
        "participants": {"clientSha256": args.client_sha256, "daemonSha256": args.daemon_sha256,
                         "relaySha256": args.relay_sha256},
        "fileIdentityInitialized": True,
        "nonOnionRejected": True,
        "relayStoredSignedKeyPackageForIdentity": True,
        "providerPhase": "provider_active",
        "samePublicationAfterRestart": True,
        "providerStopped": True,
        "ownedRuntimeRemoved": True,
        "isolatedWorkspaceRemoved": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("client", "daemon", "relay", "isolation", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    for name in ("client", "daemon", "relay"):
        parser.add_argument(f"--{name}-sha256", required=True)
    args = parser.parse_args()
    args.receipt.write_text(json.dumps(observe(args), indent=2) + "\n")


if __name__ == "__main__":
    main()
