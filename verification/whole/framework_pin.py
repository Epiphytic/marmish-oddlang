"""Expose the native baseline checker to the historical Python replay entry points."""
from contextlib import contextmanager
from functools import lru_cache
from pathlib import Path
import json
import os
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
REVISION = json.loads((HERE / "baseline-framework.json").read_bytes())


@lru_cache(maxsize=1)
def _tool():
    directory = tempfile.TemporaryDirectory(prefix="baseline-checker-")
    env = os.environ.copy()
    for key in ("RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"):
        env.pop(key, None)
    subprocess.run(["cargo", "+1.97.1", "build", "--quiet", "--locked", "--offline",
                    "--manifest-path", HERE / "canonical-public-data/Cargo.toml",
                    "--bin", "baseline-framework", "--target-dir", directory.name], env=env, check=True)
    return directory, Path(directory.name) / "debug/baseline-framework"


def _run(*args):
    result = subprocess.run([_tool()[1], *args], capture_output=True, text=True)
    if result.returncode or result.stdout.strip() != REVISION:
        raise ValueError(result.stderr.strip() or "unexpected baseline framework revision")


def verify_checkout(root):
    _run("verify", root)


@contextmanager
def build_cli(root):
    with tempfile.TemporaryDirectory(prefix="oddlang-baseline-cli-") as directory:
        cli = Path(directory) / "oddlang"
        _run("build", root, cli)
        yield cli
