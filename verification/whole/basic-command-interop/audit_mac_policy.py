"""Read back the kernel policy attached to live, pinned reference processes."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess

from isolate import SYSTEM_HOME, Workspace
from observe import BRIDGE, BUILD, copy_verified, json_command, until


FILTER_PATH = 1
FILTER_GLOBAL_NAME = 2


def processes():
    rows = subprocess.check_output(["/bin/ps", "-axo", "uid=,pid=,comm="], text=True).splitlines()
    result = {}
    for row in rows:
        fields = row.split(None, 2)
        if len(fields) == 3 and int(fields[0]) == os.geteuid():
            result[int(fields[1])] = fields[2]
    return result


def query_policy(pid):
    library = ctypes.CDLL("/usr/lib/libsandbox.1.dylib", use_errno=True)
    check = library.sandbox_check
    check.argtypes = (ctypes.c_int, ctypes.c_char_p, ctypes.c_int)
    check.restype = ctypes.c_int
    keychain = SYSTEM_HOME / "Library/Keychains/login.keychain-db"
    targets = {
        "writable_control": (b"file-write-data", FILTER_PATH, b"/dev/null", 0),
        "credential_file_read_denied": (b"file-read-data", FILTER_PATH, os.fsencode(keychain), 1),
        "credential_file_write_denied": (b"file-write-data", FILTER_PATH, os.fsencode(keychain), 1),
        "config_creation_denied": (
            b"file-write-create", FILTER_PATH, os.fsencode(SYSTEM_HOME / ".config/marmish/probe"), 1),
        "keychain_service_denied": (b"mach-lookup", FILTER_GLOBAL_NAME, b"com.apple.securityd", 1),
    }
    return {
        name: check(pid, operation, scope, ctypes.c_char_p(resource)) == expected
        for name, (operation, scope, resource, expected) in targets.items()
    }


def audit(root):
    root = root.resolve(strict=True)
    if (root.parent != Path("/private/tmp") or not re.fullmatch(r"od-bi-[a-z0-9_]+", root.name)
            or root.stat().st_uid != os.geteuid() or root.stat().st_mode & 0o777 != 0o700):
        raise ValueError("expected an owned private interop workspace")
    bridge = json.loads(BRIDGE.read_bytes())["builds"]["default"]["sha256"]
    relay = json.loads(BUILD.read_bytes())["relay"]["sha256"]
    targets = {
        str(root / "client/marmishd"): ("client-daemon", bridge["marmishd"]),
        str(root / "client/marmish"): ("client-cli", bridge["marmish"]),
        str(root / "server/marmishd"): ("server-daemon", bridge["marmishd"]),
        str(root / "server/marmish-worker"): ("server-worker", bridge["marmish-worker"]),
        str(root / "relay/deaddrop"): ("relay", relay),
    }
    records = {}
    for pid, image in processes().items():
        if image not in targets:
            continue
        role, expected = targets[image]
        with Path(image).open("rb") as source:
            found = hashlib.file_digest(source, "sha256").hexdigest()
        checks = query_policy(pid)
        if processes().get(pid) != image:
            continue
        if found != expected or not all(checks.values()):
            raise ValueError(f"artifact or kernel policy did not verify for {role}")
        records[role] = {"binary_sha256": found, "kernel_policy": checks}
    if not {"client-daemon", "server-daemon", "relay"} <= records.keys():
        raise ValueError("the exact three reference owners must be running")
    return {"schema": 1, "participants": records, "scope": "macos-arm64-live-reference-policy"}



def diagnostic(binaries, relay_binary):
    """Keep the exact original owners live at a test-owned loopback TLS listener."""
    bridge = json.loads(BRIDGE.read_bytes())["builds"]["default"]["sha256"]
    relay_pin = json.loads(BUILD.read_bytes())["relay"]["sha256"]
    with socket.socket() as listener, Workspace() as workspace:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        endpoint = f"wss://127.0.0.1:{listener.getsockname()[1]}/"
        relay = workspace.instance("relay")
        staged_relay = relay.root / "deaddrop"
        copy_verified(relay_binary, staged_relay, relay_pin)
        relay.allow_executables((staged_relay,))
        children = [relay.start([staged_relay, "debug", "--bind", "127.0.0.1:0",
                                 "--data-dir", relay.root / "data"], stdin=subprocess.DEVNULL,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)]
        for role in ("server", "client"):
            peer = workspace.instance(role)
            for name, checksum in bridge.items():
                copy_verified(binaries / name, peer.root / name, checksum)
            peer.allow_executables(tuple(peer.root / name for name in bridge))
            options = ["--state-dir", peer.root / "state", "--identity", role,
                       "--key-storage", "file", "--tor", "off", "--relay", endpoint]
            setup = json_command(peer, [peer.root / "marmish", *options, "setup",
                                        "--role", role, "--no-grants"])
            if setup["backend"] != "file" or not setup["identity_created"]:
                raise ValueError("disposable setup not verified")
            children.append(peer.start([peer.root / "marmishd", *options, "--foreground", "daemon"],
                                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                       stderr=subprocess.DEVNULL))

        def verify():
            if any(child.poll() is not None for child in children):
                raise ValueError("owned diagnostic participant exited")
            return audit(workspace.root) if len(set(processes().values()).intersection({
                str(workspace.root / "client/marmishd"), str(workspace.root / "server/marmishd"),
                str(staged_relay),
            })) == 3 else None

        result = until(verify, 10, "live sandbox policy")
    if not workspace.cleanup_verified or workspace.root.exists():
        raise ValueError("diagnostic cleanup failed")
    return dict(result, scope="macos-arm64-loopback-native-policy", workspace_removed=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="mode", required=True)
    live = subcommands.add_parser("live")
    live.add_argument("workspace", type=Path)
    local = subcommands.add_parser("diagnostic")
    local.add_argument("original_bin_dir", type=Path)
    local.add_argument("relay", type=Path)
    args = parser.parse_args()
    result = audit(args.workspace) if args.mode == "live" else diagnostic(args.original_bin_dir, args.relay)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
