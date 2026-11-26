#!/usr/bin/env python3
"""Record the pinned client handoff to a test-owned Unix socket."""

import argparse
import json
from pathlib import Path
import platform
import socket
import struct
import subprocess
import tempfile

from build import ASSEMBLY, TOOLS, read, sha


HERE = Path(__file__).resolve().parent
REFERENCE = HERE.parent / "bridge/aarch64-apple-darwin.json"
ACCOUNT = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
NPUB = "npub10xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqpkge6d"
RELAY = "wss://localhost:31337/"
STATUS = b'{"operation":"status"}'
PREPARE = b'{"operation":"command_prepare","remote":"' + ACCOUNT.encode() + b'","relays":["' + RELAY.encode() + b'"]}'
INVOCATIONS = b'{"operation":"command_invocations","after":null,"limit":64}'
READY = b'{"ok":true,"result":{"state":"running"},"error":null}'
CAPACITY = b'{"ok":true,"result":null,"error":null,"command":{"state":"failure","category":"command_room_capacity","handle":null}}'
EMPTY = b'{"ok":true,"result":null,"error":null,"command":{"state":"invocations","entries":[],"next":null}}'
CASES = {
    "foregroundSubmit": (["--stdin-none", NPUB, "--", "/usr/bin/true"],
                         ((PREPARE, CAPACITY),), b"marmish: command_room_capacity\n"),
    "detachedSubmit": (["--detach", "--stdin-none", NPUB, "--", "/usr/bin/true"],
                       ((PREPARE, CAPACITY),), b"marmish: command_room_capacity\n"),
    "wait": (["wait", "--latest", NPUB], ((INVOCATIONS, EMPTY),), b"marmish: command_not_found\n"),
    "malformedPrepareReply": (["--stdin-none", NPUB, "--", "/usr/bin/true"],
                              ((PREPARE, b"{"),), b"marmish: ipc_frame_json\n"),
}


def frame(body):
    return struct.pack(">I", len(body)) + body


def exchange(listener, request, response):
    connection, _ = listener.accept()
    with connection, connection.makefile("rb") as reader:
        connection.settimeout(3)
        if reader.read(4) != frame(request)[:4] or reader.read(len(request)) != request:
            raise ValueError("owner request bytes differ")
        if reader.read(1):
            raise ValueError("owner request was not half-closed")
        connection.sendall(frame(response))
        connection.shutdown(socket.SHUT_WR)


def config(root):
    profile = root / "default"
    root.mkdir(mode=0o700)
    profile.mkdir(mode=0o700)
    path = profile / "config.json"
    path.write_text(json.dumps({
        "version": 1, "role": "client", "relays": [RELAY], "tor": "off",
        "trusted_remotes": [NPUB], "execution_clients": [],
        "command_policy": {"version": 1}, "cwd": str(root), "limits": {}, "aliases": {},
    }))
    path.chmod(0o600)


def observe(binary, workspace, argv, steps, expected_error):
    state = workspace / "state"
    config(state)
    path = workspace / "owner"
    sandbox, option, policy = TOOLS.isolated(workspace, (binary.parent,))
    policy += f"(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))"
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    env = {"HOME": str(workspace), "TMPDIR": str(workspace), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
    transcript = ((STATUS, READY), *steps)
    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(str(path))
        listener.listen(1)
        listener.settimeout(3)
        command = [sandbox, option, policy, binary, "--state-dir", state, "--socket", path,
                   "--no-autostart", *argv]
        with subprocess.Popen(command, cwd=workspace, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as cli:
            try:
                for request, reply in transcript:
                    exchange(listener, request, reply)
                stdout, stderr = cli.communicate(timeout=3)
                listener.setblocking(False)
                try:
                    unexpected, _ = listener.accept()
                except BlockingIOError:
                    pass
                else:
                    with unexpected:
                        raise ValueError("unexpected extra local owner request")
            finally:
                if cli.poll() is None:
                    cli.kill()
                    cli.communicate(timeout=3)
    if (cli.returncode, stdout, stderr) != (255, b"", expected_error):
        raise ValueError(f"unexpected reference exit or streams: {cli.returncode}, {stdout!r}, {stderr!r}")
    return {"argv": argv, "exit": cli.returncode, "stdoutHex": stdout.hex(), "stderrHex": stderr.hex(),
            "unixRequestsHex": [frame(request).hex() for request, _ in transcript],
            "unixRepliesHex": [frame(reply).hex() for _, reply in transcript]}


def verify(binary):
    pins = read(REFERENCE)
    expected = pins["builds"]["default"]["sha256"]["marmish"]
    if pins["commit"] != ASSEMBLY["ORIGINAL"] or binary.name != "marmish" or sha(binary) != expected:
        raise ValueError("wrong pinned upstream client executable")
    results = {}
    with tempfile.TemporaryDirectory(prefix="odd-client-owner-", dir="/private/tmp") as directory:
        root = Path(directory)
        for index, (name, case) in enumerate(CASES.items()):
            work = root / str(index)
            work.mkdir(mode=0o700)
            results[name] = observe(binary, work, *case)
    return {
        "format": "marmish-basic-command-local-owner-reference-v1",
        "upstreamRevision": pins["commit"],
        "upstreamClientSha256": expected,
        "releaseReceiptSha256": sha(REFERENCE),
        "probeSha256": sha(Path(__file__)),
        "workspaceRemoved": not root.exists(),
        "cases": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream_client", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        parser.error("requires native macOS ARM64")
    result = verify(args.upstream_client)
    with args.receipt.open("x") as output:
        output.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
