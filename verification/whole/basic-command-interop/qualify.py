"""Pass unmodified process streams to the native command-output verifier."""
import atexit
from dataclasses import dataclass
from functools import lru_cache
import json
from pathlib import Path
import struct
import subprocess
from tempfile import TemporaryDirectory


@lru_cache(maxsize=1)
def prepare_verifier():
    target = TemporaryDirectory(prefix="command-output-check-")
    atexit.register(target.cleanup)
    manifest = Path(__file__).resolve().parents[1] / "canonical-public-data/Cargo.toml"
    subprocess.run(["cargo", "+1.97.1", "build", "--quiet", "--locked", "--offline",
                    "--manifest-path", manifest, "--bin", "check-command-output",
                    "--target-dir", target.name], check=True)
    return Path(target.name) / "debug/check-command-output"


def _check(args, *outputs):
    frames = bytearray()
    for output in outputs:
        frames.extend(struct.pack(">iQQ", output.status, len(output.stdout), len(output.stderr)))
        frames.extend(output.stdout)
        frames.extend(output.stderr)
    result = subprocess.run([prepare_verifier(), *args], input=frames, capture_output=True)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return result.stdout


@dataclass(frozen=True)
class Output:
    status: int
    stdout: bytes
    stderr: bytes

    def compare(self, reference):
        _check(["compare"], self, reference)

    def receipt(self):
        return json.loads(_check(["receipt"], self))


def detached_handle(submit, server_hex):
    return _check(["handle", server_hex], submit)


def finite_goodput(output, expected, elapsed_ns):
    if type(elapsed_ns) is not int:
        raise ValueError("invalid finite benchmark duration")
    return json.loads(_check(["goodput", str(elapsed_ns)], output, expected))
