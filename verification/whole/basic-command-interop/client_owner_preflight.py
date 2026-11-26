#!/usr/bin/env python3
"""Record the pinned client handoff to a test-owned Unix socket."""

import argparse
import errno
import json
from pathlib import Path
import platform
import re
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
ROOM = "123456789abc0123456789abcdef0123"
LOCAL_ACCOUNT = "c6047f9441ed7d6d3045406e95c07cd85a778e4b8cef3ca7abac09b95c709ee5"
ROOM_READY = (
    b'{"ok":true,"result":null,"error":null,"command":{"state":"ready","session":"'
    + ROOM.encode() + b'","local_account":"' + LOCAL_ACCOUNT.encode() + b'"}}'
)
LOCAL_BUSY = b'{"ok":true,"result":null,"error":null,"command":{"state":"local_busy"}}'
STDIN = b"\x00\xff\n"
SUBMIT_TEMPLATE = (
    b'{"operation":"command_submit","remote":"' + ACCOUNT.encode() + b'","relays":["'
    + RELAY.encode() + b'"],"session":"' + ROOM.encode()
    + b'","request_id":"{request}","submit":{"argv":["L3Vzci9iaW4vdHJ1ZQ==","cGnDsWF0YQ=="],'
      b'"invocation":"{invocation}","stdin":"AP8K"}}'
)
REQUEST_ID = b"[0-9a-f]{32}"
SUBMIT_PATTERN = re.compile(
    re.escape(SUBMIT_TEMPLATE)
      .replace(re.escape(b"{request}"), b"(" + REQUEST_ID + b")")
      .replace(re.escape(b"{invocation}"), b"(" + REQUEST_ID + b")")
)

CASES = {
    "foregroundSubmit": (["--stdin-none", NPUB, "--", "/usr/bin/true"],
                         ((PREPARE, CAPACITY),), b"marmish: command_room_capacity\n"),
    "detachedSubmit": (["--detach", "--stdin-none", NPUB, "--", "/usr/bin/true"],
                       ((PREPARE, CAPACITY),), b"marmish: command_room_capacity\n"),
    "wait": (["wait", "--latest", NPUB], ((INVOCATIONS, EMPTY),), b"marmish: command_not_found\n"),
    "malformedPrepareReply": (["--stdin-none", NPUB, "--", "/usr/bin/true"],
                              ((PREPARE, b"{"),), b"marmish: ipc_frame_json\n"),
    "readyForegroundSubmit": (["--stdin", NPUB, "--", "/usr/bin/true", "piñata"],
                              ((PREPARE, ROOM_READY), (SUBMIT_PATTERN, LOCAL_BUSY)),
                              b"marmish: local_busy\n", STDIN),
    "readyDetachedSubmit": (["--detach", "--stdin", NPUB, "--", "/usr/bin/true", "piñata"],
                            ((PREPARE, ROOM_READY), (SUBMIT_PATTERN, LOCAL_BUSY)),
                            b"marmish: local_busy\n", STDIN),
}


def frame(body):
    return struct.pack(">I", len(body)) + body


def exchange(listener, request, response):
    connection, _ = listener.accept()
    with connection, connection.makefile("rb") as reader:
        connection.settimeout(3)
        header = reader.read(4)
        if len(header) != 4:
            raise ValueError("missing local owner request header")
        length, = struct.unpack(">I", header)
        if not 0 < length <= 4096:
            raise ValueError("unexpected local owner request length")
        body = reader.read(length)
        if len(body) != length:
            raise ValueError("incomplete local owner request")
        dynamic_ids = ()
        if isinstance(request, bytes):
            if body != request:
                raise ValueError("owner request bytes differ")
        else:
            match = request.fullmatch(body)
            if match is None or match[1] == match[2]:
                raise ValueError("invalid command submit frame or ID bindings")
            dynamic_ids = match.groups()
            body = SUBMIT_TEMPLATE.replace(b"{request}", b"1" * 32).replace(b"{invocation}", b"2" * 32)
        if reader.read(1):
            raise ValueError("owner request was not half-closed")
        connection.sendall(frame(response))
        try:
            connection.shutdown(socket.SHUT_WR)
        except OSError as error:
            if error.errno != errno.ENOTCONN:
                raise
        return frame(body).hex(), dynamic_ids


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


def observe(binary, workspace, argv, steps, expected_error, input_bytes=b""):
    state = workspace / "state"
    config(state)
    path = workspace / "owner"
    sandbox, option, policy = TOOLS.isolated(workspace, (binary.parent,))
    policy += f"(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))"
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    env = {"HOME": str(workspace), "TMPDIR": str(workspace), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
    transcript = ((STATUS, READY), *steps)
    requests, ids = [], []
    with socket.socket(socket.AF_UNIX) as listener, tempfile.TemporaryFile(dir=workspace) as stdin:
        stdin.write(input_bytes)
        stdin.seek(0)
        listener.bind(str(path))
        listener.listen(1)
        listener.settimeout(3)
        command = [sandbox, option, policy, binary, "--state-dir", state, "--socket", path,
                   "--no-autostart", *argv]
        with subprocess.Popen(command, cwd=workspace, env=env, stdin=stdin,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as cli:
            try:
                for request, reply in transcript:
                    received, dynamic_ids = exchange(listener, request, reply)
                    requests.append(received)
                    ids.extend(dynamic_ids)
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
    receipt = {"argv": argv, "exit": cli.returncode, "stdoutHex": stdout.hex(), "stderrHex": stderr.hex(),
               "unixRequestsHex": requests, "unixRepliesHex": [frame(reply).hex() for _, reply in transcript]}
    if ids:
        receipt["stdinHex"] = input_bytes.hex()
        receipt["normalizedIdFields"] = ["request_id", "submit.invocation"]
    return receipt, ids


def verify(binary):
    pins = read(REFERENCE)
    expected = pins["builds"]["default"]["sha256"]["marmish"]
    if pins["commit"] != ASSEMBLY["ORIGINAL"] or binary.name != "marmish" or sha(binary) != expected:
        raise ValueError("wrong pinned upstream client executable")
    results = {}
    observed_ids = []
    with tempfile.TemporaryDirectory(prefix="odd-client-owner-", dir="/private/tmp") as directory:
        root = Path(directory)
        for index, (name, case) in enumerate(CASES.items()):
            work = root / str(index)
            work.mkdir(mode=0o700)
            results[name], identifiers = observe(binary, work, *case)
            observed_ids.extend(identifiers)
    if len(observed_ids) != 4 or len(set(observed_ids)) != len(observed_ids):
        raise ValueError("command submit IDs were not distinct across cases")
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
