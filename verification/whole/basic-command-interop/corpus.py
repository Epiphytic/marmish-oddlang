#!/usr/bin/python3
"""Fixed cases and independent expectations for the pinned upstream fixture."""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import sys


@dataclass(frozen=True)
class Case:
    name: str
    stdin: bytes
    stdout: bytes
    stderr: bytes
    status: int
    args: tuple = ()


SHORT_CASES = (
    Case("success", b"hello", b"done\n", b"", 0),
    Case("separate-streams", b"\x00\xfe\n", b"stdout\x00\xff", b"stderr\x00\xfe", 0),
    Case("empty", b"", b"", b"", 0),
    Case("trailing-newlines", b"next\n", b"one\r\n\n", b"two", 0),
    Case("nonzero", b"finish", b"partial\n", b"controlled error\n", 7),
)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def finite_case(repetition, vector=0):
    if repetition not in range(4) or vector != 0:
        raise ValueError("unrecognized finite benchmark fixture")
    key = hashlib.sha256(f"oddlang finite benchmark vector 0 round {repetition}".encode()).digest()
    nonce = key.hex().encode()
    argv = (nonce, b"0", b"", b"""quoted ' " \\ $() ; * ?""", "line\n雪".encode(), b"e\xffd")

    def byte_vector(length, multiplier, label):
        return bytes((index * multiplier + key[(index // 256) % 32] + label) % 256
                     for index in range(length))

    return Case("finite", byte_vector(16384, 1, 0), byte_vector(131072, 17, 23),
                byte_vector(131072, 17, 149), 7, argv)


def verify_output(case, result):
    for field in ("status", "stdout", "stderr"):
        if getattr(case, field) != getattr(result, field):
            raise ValueError(f"{case.name}: {field} mismatch")


def expected_receipt(case, executable, args):
    argv = (os.fsencode(executable), *args)
    encoded = b"".join(len(arg).to_bytes(4, "big") + arg for arg in argv)
    receipt = {
        "schema": 1,
        "executable_sha256": digest(executable.read_bytes()),
        "argv_sha256": digest(encoded),
        "stdin_bytes": len(case.stdin),
        "stdin_sha256": digest(case.stdin),
        "stdout_bytes": len(case.stdout),
        "stdout_sha256": digest(case.stdout),
        "stderr_bytes": len(case.stderr),
        "stderr_sha256": digest(case.stderr),
        "exit": case.status,
    }
    if case.name == "finite":
        return dict(receipt, nonce=args[0].decode(), vector=int(args[1]))
    return dict(receipt, nonce=args[1].decode(), case=case.name)


def main():
    # Only the fixed, short cases can execute through this fixture.
    if len(sys.argv) != 3 or not re.fullmatch(r"[0-9a-f]{64}", sys.argv[2]):
        return 125
    case = next((case for case in SHORT_CASES if case.name == sys.argv[1]), None)
    if case is None or sys.stdin.buffer.read(len(case.stdin) + 1) != case.stdin:
        return 125
    executable = Path(sys.argv[0]).resolve(strict=True)
    path = executable.parent / "receipts" / f"{sys.argv[2]}.receipt"
    sys.stdout.buffer.write(case.stdout)
    sys.stdout.buffer.flush()
    sys.stderr.buffer.write(case.stderr)
    sys.stderr.buffer.flush()
    with path.open("x") as stream:
        json.dump(expected_receipt(case, executable, tuple(map(os.fsencode, sys.argv[1:]))), stream)
    return case.status


if __name__ == "__main__":
    sys.exit(main())
