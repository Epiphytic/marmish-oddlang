#!/usr/bin/env python3
"""Observe a restarted client reading a fixture-retained pending poll."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
import signal
import socket
import struct
import subprocess
from tempfile import TemporaryDirectory

import client_progress_reference as progress

ref = progress.ref
INITIAL = (8, 9)
BEFORE_STOP = ("status", "command_unconsumed", "command_cursors", "command_poll")
RECOVERY = ("status", "command_unconsumed", "command_response", "command_cursors", "command_consume")


class RetainedPoll(progress.PollOwner):
    def __init__(self):
        super().__init__(widths=(8, 9))
        self.start = INITIAL
        self.retained = None

    def exchange(self, operation, actual, *, bad_digest=False):
        if self.retained is not None:
            if operation == "command_unconsumed":
                request = dict(operation=operation, handle=self.handle, limit=64)
                reply = ref.local("entries", entries=[
                    dict(request_id=self.poll_id, op="poll", sha256=self.poll_response_digest)
                ])
                return ref.compact(request), ref.compact(reply)
            if operation == "command_response":
                request = dict(operation=operation, handle=self.handle, request_id=self.poll_id)
                return ref.compact(request), self.retained
        return super().exchange(operation, actual, bad_digest=bad_digest)


def stop_with_poll(binary, work, owner, handle):
    path = work / "o"
    sandbox, option, policy = ref.TOOLS.isolated(work, (binary.parent,))
    policy += f'(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))'
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    policy += f'(deny process-exec)(allow process-exec (literal {json.dumps(str(binary))}))'
    env = dict(HOME=str(work), TMPDIR=str(work), PATH="/usr/bin:/bin", LC_ALL="C")
    argv = [sandbox, option, policy, binary, "--state-dir", work / "state", "--socket", path,
            "--no-autostart", "wait", "--handle", handle.decode()]
    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(str(path))
        listener.listen(1)
        listener.settimeout(4)
        with subprocess.Popen(argv, cwd=work, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as client:
            try:
                for operation in BEFORE_STOP:
                    with listener.accept()[0] as connection, connection.makefile("rb") as reader:
                        connection.settimeout(4)
                        size, = struct.unpack(">I", reader.read(4))
                        if not 0 < size <= 65536:
                            raise ValueError("invalid pending-poll frame size")
                        data = reader.read(size)
                        if len(data) != size or reader.read(1):
                            raise ValueError("invalid pending-poll frame or half-close")
                        expected, reply = owner.exchange(operation, json.loads(data))
                        if data != expected:
                            raise ValueError(f"unexpected frame at {operation}")
                        if operation == "command_poll":
                            owner.retained = reply
                            client.kill()
                        else:
                            connection.sendall(ref.frame(reply))
                stdout, stderr = client.communicate(timeout=4)
                if (client.returncode, stdout, stderr) != (-signal.SIGKILL, b"", b""):
                    raise ValueError("first wait did not stop before delivery")
            finally:
                if client.poll() is None:
                    client.kill()
                    client.wait(timeout=4)
    path.unlink()


def observe(binary, mode):
    owner = RetainedPoll()
    with TemporaryDirectory(prefix="opr-", dir="/private/tmp") as directory:
        work = Path(directory)
        command = ["--detach", "--stdin-none", ref.NPUB, "--", "/usr/bin/true"]
        status, out, err = progress.run_client(binary, work, owner, command, ref.SUBMIT)
        handle = ref.compact(owner.handle)
        line = b"marmish handle: " + handle + b"\n"
        if (status, out, err) != (0, b"", line):
            raise ValueError("detached handle bytes differ")
        manifest = work / "state/default/profile.json"
        manifest.write_bytes(ref.compact(dict(format=2, name="default", phase="ready",
                                              public_key=ref.LOCAL_ACCOUNT)))
        manifest.chmod(0o600)
        stop_with_poll(binary, work, owner, handle)
        operations = RECOVERY if mode == "upstream" else RECOVERY[:2]
        status, out, err = progress.run_client(
            binary, work, owner, ["wait", "--handle", handle.decode()], operations,
        )
        if mode == "upstream":
            expected = (7, ref.STDOUT[INITIAL[0]:], ref.STDERR[INITIAL[1]:])
        else:
            expected = (255, b"", line + b"marmish: command_unconsumed\n")
        if (status, out, err) != expected:
            raise ValueError("restarted wait differs from expected local outcome")
        committed = owner.start
        if committed != (tuple(map(len, owner.outputs)) if mode == "upstream" else INITIAL):
            raise ValueError("unexpected cursor consumption")
    return dict(mode=mode, beforeStop=BEFORE_STOP, restartOperations=operations,
                initialCursors=INITIAL, finalCursors=committed, exit=status,
                stdoutHex=out.hex(), stderrHex=err.hex() if mode == "upstream" else None,
                exactHandleOnRefusal=mode == "candidate",
                fixtureRetainedAnswer=True, workspaceRemoved=not work.exists())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("candidate", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("build_receipt", type=Path)
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    build = json.loads(args.build_receipt.read_bytes())
    upstream = json.loads((Path(__file__).parent / "upstream-local-progress-aarch64-apple-darwin.json").read_bytes())
    actors = (("upstream", args.upstream, upstream["clientSha256"]),
              ("candidate", args.candidate, build["artifacts"]["marmish"]["sha256"]))
    observations = []
    for mode, binary, expected_sha in actors:
        if ref.sha(binary) != expected_sha:
            raise ValueError(f"{mode} binary digest differs")
        observations.append(dict(clientSha256=expected_sha, **observe(binary, mode)))
    receipt = dict(format="marmish-local-pending-poll-reference-v1",
                   buildReceiptSha256=sha256(args.build_receipt.read_bytes()).hexdigest(),
                   fixtureSha256=ref.sha(Path(__file__)), observations=observations)
    with args.receipt.open("x") as output:
        output.write(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
