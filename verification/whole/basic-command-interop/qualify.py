"""Byte-level comparison used by the mixed-peer basic-command runner."""

from dataclasses import dataclass
from hashlib import sha256
import re


@dataclass(frozen=True)
class Output:
    status: int
    stdout: bytes
    stderr: bytes

    def compare(self, reference):
        for name in ("status", "stdout", "stderr"):
            if getattr(self, name) != getattr(reference, name):
                raise ValueError(f"command {name} differs from the reference")

    def receipt(self):
        def stream(data):
            return {"bytes": len(data), "sha256": sha256(data).hexdigest()}

        return {"status": self.status, "stdout": stream(self.stdout), "stderr": stream(self.stderr)}


# Only the four run-specific values vary. Field order, punctuation, and LF
# are observable CLI bytes. The remote field is the server's hex public key.
HANDLE_LINE = re.compile(
    rb'marmish handle: (?P<encoded>\{"format":1,"profile":"client","remote":"(?P<remote>[0-9a-f]{64})",'
    rb'"session":"(?P<session>[0-9a-f]{32})","invocation":"(?P<invocation>[0-9a-f]{32})",'
    rb'"submit_sha256":"(?P<digest>[0-9a-f]{64})"\})\n'
)


def detached_handle(submit, server_hex):
    """Return the exact argument to wait after validating detach output."""
    handle = HANDLE_LINE.fullmatch(submit.stderr)
    if submit.status != 0 or submit.stdout or handle is None:
        raise ValueError("invalid detached response")
    if handle["remote"] != server_hex.encode("ascii"):
        raise ValueError("detached response names a different server")
    return handle["encoded"]


def finite_goodput(output, expected, elapsed_ns):
    """Measure full submit-and-wait time after checking bytes and status."""
    output.compare(expected)
    size = len(output.stdout) + len(output.stderr)
    if size != 262144:
        raise ValueError("unexpected finite benchmark payload size")
    if type(elapsed_ns) is not int or not 0 < elapsed_ns <= 330_000_000_000:
        raise ValueError("invalid finite benchmark duration")
    return size * 1_000_000_000 / elapsed_ns
