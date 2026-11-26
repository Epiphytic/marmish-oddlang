#!/usr/bin/env python3
"""Witness the pinned client consuming binary replies from a test-owned local actor."""

import argparse
from base64 import b64encode
from hashlib import sha256
import json
from pathlib import Path
import re
import socket
import struct
import subprocess
import tempfile

from client_owner_preflight import (
    ACCOUNT, LOCAL_ACCOUNT, NPUB, PREPARE, REFERENCE, ROOM, ROOM_READY, READY, STATUS,
    TOOLS, config, frame, read, sha,
)


HERE = Path(__file__).resolve().parent
PROTOCOL = "marmish-command/1"
STDOUT = b"fixture stdout\x00\n"
STDERR = b"fixture stderr\xff\n"
IDENTIFIER = re.compile(r"[0-9a-f]{32}\Z")
SUBMIT = ("status", "command_prepare", "command_submit", "command_unconsumed",
          "command_response", "command_consume")
POLL = ("command_cursors", "command_poll", "command_consume")


def compact(body, *, sort_keys=False):
    return json.dumps(body, separators=(",", ":"), sort_keys=sort_keys).encode()


def packet(operation, kind, request_id, body):
    return compact(dict(body=body, client=LOCAL_ACCOUNT, id=request_id, op=operation,
                        protocol=PROTOCOL, server=ACCOUNT, session=ROOM, type=kind), sort_keys=True)


def local(state, **fields):
    return dict(ok=True, result=None, error=None, command=dict(state=state, **fields))


def checked_id(value, used):
    if not isinstance(value, str) or not IDENTIFIER.fullmatch(value) or value in used:
        raise ValueError("invalid or repeated native command identifier")
    used.add(value)
    return value


def status(invocation, finished):
    def channel(body):
        length = len(body) if finished else 0
        return dict(closed=finished, gaps=[], produced=length, retained=length)

    return dict(admission="engine", authority_valid=True, cancel_requested=False, engine_known=True,
                input=dict(accepted=0, delivered=0, eof_sent=True, health="closed", length=0),
                invocation=invocation, outcome=dict(kind="code", value=7) if finished else None,
                state="exited" if finished else "running", stderr=channel(STDERR), stdout=channel(STDOUT),
                warning=None)


class Owner:
    def __init__(self):
        self.handle = None
        self.used_ids = set()
        self.submit_consumed = False

    def answer(self, operation, request_id, request_sha, result):
        response = packet(operation, "response", request_id,
                          dict(error=None, ok=True, request_sha256=request_sha, result=result))
        checksum = sha256(response).hexdigest()
        return local("answer", handle=self.handle, request_id=request_id,
                     response_base64=b64encode(response).decode(), response_sha256=checksum), checksum

    def exchange(self, operation, actual, *, bad_digest=False):
        handle = self.handle
        if operation == "status":
            return STATUS, READY
        if operation == "command_prepare":
            return PREPARE, ROOM_READY
        if operation == "command_submit":
            self.submit_id = checked_id(actual["request_id"], self.used_ids)
            self.invocation = checked_id(actual["submit"]["invocation"], self.used_ids)
            payload = dict(argv=["L3Vzci9iaW4vdHJ1ZQ=="], invocation=self.invocation, stdin="")
            expected = dict(operation=operation, remote=ACCOUNT,
                            relays=["wss://localhost:31337/"], session=ROOM,
                            request_id=self.submit_id, submit=payload)
            digest = sha256(packet("submit", "request", self.submit_id, payload)).hexdigest()
            self.handle = dict(format=1, profile="default", remote=ACCOUNT,
                               session=ROOM, invocation=self.invocation, submit_sha256=digest)
            result = dict(disposition="accepted", status=status(self.invocation, False))
            self.submit_answer, self.submit_response_digest = self.answer("submit", self.submit_id, digest, result)
            return compact(expected), compact(local("local_queued", handle=self.handle))
        if operation == "command_unconsumed":
            entries = [] if self.submit_consumed else [
                dict(request_id=self.submit_id, op="submit", sha256=self.submit_response_digest)
            ]
            return compact(dict(operation=operation, handle=handle, limit=64)), compact(local("entries", entries=entries))
        if operation == "command_response":
            reply = self.submit_answer
            if bad_digest:
                reply = local("answer", handle=handle, request_id=self.submit_id,
                              response_base64=reply["command"]["response_base64"], response_sha256="0" * 64)
            return compact(dict(operation=operation, handle=handle, request_id=self.submit_id)), compact(reply)
        if operation == "command_cursors":
            return compact(dict(operation=operation, handle=handle)), compact(local("cursors", stdout=0, stderr=0))
        if operation == "command_poll":
            self.poll_id = checked_id(actual["request_id"], self.used_ids)
            expected = dict(operation=operation, handle=handle, request_id=self.poll_id,
                            stdout_cursor=0, stderr_cursor=0, max_bytes=16384)
            body = dict(invocation=self.invocation, max_bytes=16384, stderr_cursor=0, stdout_cursor=0)
            digest = sha256(packet("poll", "request", self.poll_id, body)).hexdigest()
            result = dict(status=status(self.invocation, True),
                          stdout=dict(bytes=b64encode(STDOUT).decode(), next_cursor=len(STDOUT)),
                          stderr=dict(bytes=b64encode(STDERR).decode(), next_cursor=len(STDERR)))
            reply, self.poll_response_digest = self.answer("poll", self.poll_id, digest, result)
            return compact(expected), compact(reply)
        if operation == "command_consume":
            poll = self.submit_consumed
            expected = dict(operation=operation, handle=handle,
                            request_id=self.poll_id if poll else self.submit_id,
                            response_sha256=self.poll_response_digest if poll else self.submit_response_digest,
                            expected_cursors=dict(stdout=0, stderr=0) if poll else None)
            self.submit_consumed = True
            return compact(expected), compact(local("consumed", stdout=len(STDOUT) if poll else 0,
                                                    stderr=len(STDERR) if poll else 0,
                                                    finish_request_id=None, finish_confirmed=poll))
        raise ValueError(f"unexpected receipt operation {operation}")


def run_client(binary, work, owner, argv, operations, *, bad_digest=False):
    state = work / "state"
    if not state.exists():
        config(state)
    path = work / "o"
    sandbox, option, policy = TOOLS.isolated(work, (binary.parent,))
    policy += f"(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))"
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    env = {"HOME": str(work), "TMPDIR": str(work), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(str(path))
        listener.listen()
        listener.settimeout(4)
        args = [sandbox, option, policy, binary, "--state-dir", state, "--socket", path,
                "--no-autostart", *argv]
        with subprocess.Popen(args, env=env, cwd=work, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, stdin=subprocess.DEVNULL) as client:
            try:
                for operation in operations:
                    with listener.accept()[0] as connection, connection.makefile("rb") as reader:
                        connection.settimeout(4)
                        size, = struct.unpack(">I", reader.read(4))
                        if not 0 < size <= 65536:
                            raise ValueError("invalid request frame size")
                        actual = reader.read(size)
                        if len(actual) != size or reader.read(1):
                            raise ValueError("invalid request frame or missing half-close")
                        expected, response = owner.exchange(operation, json.loads(actual), bad_digest=bad_digest)
                        if actual != expected:
                            raise ValueError(f"request bytes differ at {operation}")
                        connection.sendall(frame(response))
                stdout, stderr = client.communicate(timeout=4)
                listener.setblocking(False)
                try:
                    extra, _ = listener.accept()
                except BlockingIOError:
                    pass
                else:
                    with extra:
                        raise ValueError("unexpected extra owner request")
            finally:
                if client.poll() is None:
                    client.kill()
                    client.communicate(timeout=4)
    path.unlink()
    return client.returncode, stdout, stderr


def verify(binary):
    reference = read(REFERENCE)
    digest = reference["builds"]["default"]["sha256"]["marmish"]
    if binary.name != "marmish" or sha(binary) != digest:
        raise ValueError("wrong pinned native client")
    command = ["--stdin-none", NPUB, "--", "/usr/bin/true"]
    observations = {}
    with tempfile.TemporaryDirectory(prefix="odl-", dir="/private/tmp") as directory:
        root = Path(directory)
        work = root / "foreground"
        work.mkdir(mode=0o700)
        operations = SUBMIT + POLL
        code, out, err = run_client(binary, work, Owner(), command, operations)
        if (code, out, err) != (7, STDOUT, STDERR):
            raise ValueError("foreground binary outcome differs")
        observations["foreground"] = dict(operations=operations, exit=code, stdoutHex=out.hex(), stderrHex=err.hex())

        work = root / "detached"
        work.mkdir(mode=0o700)
        owner = Owner()
        code, out, err = run_client(binary, work, owner, ["--detach", *command], SUBMIT)
        handle_line = b"marmish handle: " + compact(owner.handle) + b"\n"
        if (code, out, err) != (0, b"", handle_line):
            raise ValueError("detached submit or handle differs")
        observations["detach"] = dict(operations=SUBMIT, exit=code, stdoutHex=out.hex(),
                                      stderrFormat="marmish handle: <exact-handle-json>\n")
        operations = ("status", "command_unconsumed") + POLL
        code, out, err = run_client(binary, work, owner,
                                    ["wait", "--handle", compact(owner.handle).decode()], operations)
        if (code, out, err) != (7, STDOUT, STDERR):
            raise ValueError("detached wait binary outcome differs")
        observations["wait"] = dict(operations=operations, exit=code, stdoutHex=out.hex(), stderrHex=err.hex())

        work = root / "corrupted"
        work.mkdir(mode=0o700)
        owner = Owner()
        operations = SUBMIT[:-1]
        code, out, err = run_client(binary, work, owner, command, operations, bad_digest=True)
        expected = b"marmish handle: " + compact(owner.handle) + b"\nmarmish: command_response\n"
        if (code, out, err) != (255, b"", expected) or owner.submit_consumed:
            raise ValueError(f"corrupted reply outcome differs: {code}, {out!r}, {err!r}")
        observations["corruptedDigest"] = dict(operations=operations, exit=code, stdoutHex=out.hex(),
                                               stderrFormat="marmish handle: <exact-handle-json>\nmarmish: command_response\n",
                                               consumed=False)
    return dict(format="marmish-basic-command-local-delivery-reference-v1", upstreamRevision=reference["commit"],
                upstreamClientSha256=digest, fixtureSha256=sha(Path(__file__)),
                allRequestFramesCompared=True, workspaceRemoved=not root.exists(), observations=observations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original_marmish", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    result = verify(args.original_marmish)
    with args.receipt.open("x") as file:
        file.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
