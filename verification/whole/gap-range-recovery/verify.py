"""Recover the native gap record and replay the archived original data witnesses."""

import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
FRAMEWORK = "9923e6664899de66915ed3a94e417f261ffed401"
RECORD = "engine-gap-range-recovered-v1"
ID = "urn:marmish:engine:output-gap:"
BUILD_RUSTC = "31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd"
RUSTC = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HOSTS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")


def pinned(path, commit):
    recorded = subprocess.check_output(
        ["git", "show", f"{commit}:{path.relative_to(ROOT)}"], cwd=ROOT
    )
    if path.read_bytes() != recorded:
        raise ValueError(f"pinned verification input differs: {path.name}")
    return recorded


prior_helper = HERE.parent / "gap-range-record/verify.py"
# The current helper retrieves the original observer from its pinned Git revision.
pinned(prior_helper, "393a561aca0103148bcc443c54c989309ac65bee")
PRIOR = runpy.run_path(str(prior_helper))
run, sha, dump, clean = (PRIOR[key] for key in ("run", "sha", "dump", "clean"))
PIN, MODEL, ENV, BASELINE = (PRIOR[key] for key in ("PIN", "MODEL", "ENV", "BASELINE"))


def read(path):
    return json.loads(path.read_bytes())


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="range-verifier-") as target:
        subprocess.run([
            "cargo", "+1.97.1", "run", "--locked", "--offline",
            "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
            "--target-dir", target, "--bin", "range-contract-inputs", "--", "qualify",
            *sys.argv[1:],
        ], check=True)
