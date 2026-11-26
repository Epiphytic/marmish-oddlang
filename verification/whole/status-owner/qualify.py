#!/usr/bin/env python3
"""Compare pinned original activation status with an original-client/generated-owner pair."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import signal
import socket
import stat
import struct
import subprocess
import tempfile
import time

from project import INPUTS, REFERENCE

HERE = Path(__file__).resolve().parent
EXPECTED = {"exit": 0, "stdout": '{"ok":true,"result":{"state":"activating"},"error":null}\n', "stderr": ""}
MISSING = {"exit": 1, "stdout": "", "stderr": "marmish: local I/O failed\n"}
INVALID = b'{"ok":false,"result":null,"error":"invalid_request"}'
STATUS = b'{"operation":"status"}'


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def frame(body):
    return struct.pack(">I", len(body)) + body


def check(value, expected, name):
    if value != expected:
        raise ValueError(f"{name}: expected {expected!r}, got {value!r}")


def client(program, state, endpoint, env):
    response = subprocess.run(
        [program, "--state-dir", state, "--socket", endpoint, "status"],
        cwd=state.parent, stdin=subprocess.DEVNULL, capture_output=True,
        env=env, timeout=5, umask=0o077,
    )
    return {"exit": response.returncode, "stdout": response.stdout.decode(), "stderr": response.stderr.decode()}


def receive(endpoint, wire):
    with socket.socket(socket.AF_UNIX) as peer:
        peer.settimeout(4)
        peer.connect(str(endpoint))
        peer.sendall(wire)
        peer.shutdown(socket.SHUT_WR)
        out = bytearray()
        while chunk := peer.recv(256):
            out.extend(chunk)
            if len(out) > 4096:
                raise ValueError("status reply exceeded observer bound")
        return bytes(out)


def stop(owner):
    if owner.poll() is None:
        owner.send_signal(signal.SIGTERM)
    try:
        out, err = owner.communicate(timeout=5)
    finally:
        if owner.poll() is None:
            owner.kill()
            owner.communicate()
    return {"exit": owner.returncode, "stdout": out.decode(), "stderr": err.decode()}


def wait_endpoint(owner, candidates):
    deadline = time.monotonic() + 5
    while owner.poll() is None and time.monotonic() < deadline:
        paths = [path for path in candidates() if path.is_socket()]
        if len(paths) == 1 and stat.S_IMODE(paths[0].stat().st_mode) == 0o600:
            return paths[0]
        time.sleep(0.01)
    raise ValueError("owner failed to publish a private local endpoint")


def observe(program, original_client, root, owner_binary=None):
    root.mkdir(mode=0o700)
    state, runtime = root / "state", root / "runtime"
    for path in (state, runtime):
        path.mkdir(mode=0o700)
    env = {"LANG": "C", "PATH": "/usr/bin:/bin", "HOME": str(root), "TMPDIR": str(runtime)}
    with socket.socket() as stalled_relay:
        if owner_binary is None:
            stalled_relay.bind(("127.0.0.1", 0))
            stalled_relay.listen()
            relay = f"wss://127.0.0.1:{stalled_relay.getsockname()[1]}"
            command = [program, "--foreground", "--state-dir", state, "--key-storage", "file",
                       "--tor", "off", "--relay", relay]
            def candidates():
                return runtime.glob("mr2-*/s")
        else:
            endpoint = runtime / "s"
            command = [owner_binary, "--socket", endpoint]
            def candidates():
                return (endpoint,)
        owner = subprocess.Popen(
            command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=env, cwd=root, umask=0o077,
        )
        try:
            endpoint = wait_endpoint(owner, candidates)
            snapshot = client(original_client, state, endpoint, env)
            check(snapshot, EXPECTED, "original client activation")
            good = frame(EXPECTED["stdout"].strip().encode())
            controls = {}
            for name, request, expected in (
                ("valid-status", frame(STATUS), good),
                ("malformed-json", frame(b"{"), frame(INVALID)),
                ("duplicate-tag", frame(b'{"operation":"status","operation":"status"}'), frame(INVALID)),
                ("unexpected-field", frame(b'{"operation":"status","extra":1}'), frame(INVALID)),
                ("unknown-operation", frame(b'{"operation":"other"}'), frame(INVALID)),
                ("zero-frame", struct.pack(">I", 0), frame(INVALID)),
                ("over-limit", struct.pack(">I", 65_537), frame(INVALID)),
            ):
                reply = receive(endpoint, request)
                check(reply, expected, name)
                controls[name] = reply.hex()
        finally:
            shutdown = stop(owner)
    check(shutdown["exit"], 0, "SIGTERM process exit")
    check(endpoint.exists(), False, "SIGTERM socket removal")
    after = client(original_client, state, endpoint, env)
    check(after, MISSING, "original client after shutdown")
    return {
        "status": snapshot, "frames": controls, "shutdown": shutdown,
        "socketRemoved": True, "afterShutdown": after, "socketMode": "0600",
    }


def admission_checks(binary, root):
    root.mkdir(mode=0o700)
    occupied, stale, shared = root / "file", root / "stale", root / "shared"
    occupied.write_bytes(b"foreign")
    with socket.socket(socket.AF_UNIX) as peer:
        peer.bind(str(stale))
        stale_inode = stale.stat().st_ino
    shared.mkdir()
    shared.chmod(0o755)
    cases = {
        "occupied-file": (occupied, "socket occupied"),
        "stale-socket": (stale, "socket occupied"),
        "non-private-parent": (shared / "s", "invalid input: socket_directory_permissions"),
        "long-path": (root / ("x" * 104), "invalid input: socket_path_length"),
    }
    observed = {}
    for label, (path, diagnostic) in cases.items():
        result = subprocess.run(
            [binary, "--socket", path], cwd=root, stdin=subprocess.DEVNULL,
            capture_output=True, timeout=5, umask=0o077,
            env={"HOME": str(root), "LANG": "C", "PATH": "/usr/bin:/bin"},
        )
        row = {"exit": result.returncode, "stdout": result.stdout.decode(), "stderr": result.stderr.decode()}
        check(row, {"exit": 1, "stdout": "", "stderr": f"marmish status owner: {diagnostic}\n"}, label)
        observed[label] = row
    check(occupied.read_bytes(), b"foreign", "occupied file retained")
    check(stale.stat().st_ino, stale_inode, "stale socket retained")
    return observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=lambda value: Path(value).resolve())
    parser.add_argument("candidate", type=lambda value: Path(value).resolve())
    parser.add_argument("output", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--build-receipt", type=Path)

    args = parser.parse_args()
    revision = subprocess.check_output(["git", "-C", HERE, "rev-parse", "HEAD"], text=True).strip()
    if subprocess.check_output(["git", "-C", HERE, "status", "--porcelain", "--untracked-files=all"]):
        raise ValueError("owner qualification requires a clean companion checkout")
    host = (platform.system(), platform.machine())
    targets = {("Darwin", "arm64"): "aarch64-apple-darwin", ("Linux", "aarch64"): "aarch64-unknown-linux-gnu"}
    target = targets[host]
    generated = args.candidate.parents[2]
    for name, pinned in (("Cargo.lock", "owner-Cargo.lock"), ("package.lock", "package.lock")):
        check((generated / name).read_bytes(), (INPUTS / pinned).read_bytes(), name)
    build_receipt = args.build_receipt
    if build_receipt is None:
        if target != "aarch64-apple-darwin":
            parser.error("--build-receipt is required outside macOS ARM64")
        build_receipt = HERE.parent / "bridge/aarch64-apple-darwin.json"
    provenance = json.loads(build_receipt.read_bytes())
    check(provenance["commit"], REFERENCE, "original input revision")
    hashes = provenance["builds"]["default"]["sha256"]
    for name, expected in hashes.items():
        check(fingerprint(args.original / name), expected, name)
    with tempfile.TemporaryDirectory(prefix="oso-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        client_path = args.original / "marmish"
        daemon = args.original / "marmishd"
        original = observe(daemon, client_path, root / "original")
        mixed = observe(daemon, client_path, root / "mixed", args.candidate)
        admission = admission_checks(args.candidate, root / "admission")
    for key in ("status", "frames", "socketRemoved", "afterShutdown", "socketMode"):
        check(mixed[key], original[key], key)
    original_report = json.loads(original["shutdown"]["stdout"])
    for flag in ("engine_stop_complete", "mdk_stop_complete", "runtime_cleanup_complete"):
        check(original_report[flag], True, flag)
    check(mixed["shutdown"], {"exit": 0, "stdout": "", "stderr": ""}, "local owner shutdown")
    report = {
        "platform": target, "frameworkRevision": args.framework_revision,
        "companionRevision": revision, "originalBuildReceiptSha256": fingerprint(build_receipt),
        "referenceRevision": REFERENCE, "originalBinaries": hashes,
        "candidateSha256": fingerprint(args.candidate),
        "packageManifestSha256": fingerprint(INPUTS / "package.json"),
        "packageLockSha256": fingerprint(INPUTS / "package.lock"),
        "graphSha256": fingerprint(INPUTS / "graph.json"),
        "cargoLockSha256": fingerprint(INPUTS / "owner-Cargo.lock"),
        "original": original, "reverseMixed": mixed, "candidateAdmissionControls": admission,
        "unqualified": [
            "provider activation, active and degraded snapshots",
            "installed executable, default profile, startup lease and filesystem lifecycle",
            "all non-status IPC operations and provider shutdown reports",
            "different-UID credentials, native I/O deadlines and maximum 64-client saturation",
            "externally observable stopping snapshot",
        ],
    }
    with args.output.open("x") as output:
        output.write(json.dumps(report, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
