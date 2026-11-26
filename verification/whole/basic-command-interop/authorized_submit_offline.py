#!/usr/bin/env python3
"""Exercise the generated local submit handler with a synthetic retained-room row."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
WORKSPACE = HERE.parents[2] / "oddlang/portable/workspace-authorized-owner-submit.json"
CASE = "authorized_submit_offline::local_room_selection_and_commit_receipt"


def verify(compiler, target):
    with tempfile.TemporaryDirectory(prefix="marmish-offline-submit-") as name:
        scratch = Path(name).resolve()
        generated = scratch / "generated"
        subprocess.run((compiler, "build", "native-workspace", "rust", WORKSPACE, generated),
                       check=True, timeout=60)
        candidates = [
            path for path in (generated / "marmish-native/src").glob("group_*/__projection_*.rs")
            if b"fn persist_verified_submit(" in path.read_bytes()
        ]
        if len(candidates) != 1:
            raise ValueError("expected exactly one local submit projection")
        with candidates[0].open("ab") as source:
            source.write(b"\n")
            source.write((HERE / "authorized_submit_offline.rs").read_bytes())
        env = {**os.environ, "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0"}
        result = subprocess.run(
            ("cargo", "+1.97.1", "test", "--lib", "--offline", "--locked", "--no-run",
             "--message-format=json-render-diagnostics", "--target-dir", target, "-p", "marmish-native"),
            cwd=generated, env=env, check=True, stdout=subprocess.PIPE, timeout=1200,
        )
        binaries = [
            row["executable"] for line in result.stdout.splitlines()
            if (row := json.loads(line)).get("executable") and row["profile"]["test"]
        ]
        if len(binaries) != 1:
            raise ValueError("expected one compiled local fixture")
        state = scratch / "fixture-state"
        state.mkdir(mode=0o700)
        env["MARMISH_OFFLINE_SUBMIT_FIXTURE"] = str(state)
        isolated = ("/usr/bin/sandbox-exec", "-p", "(version 1)(allow default)(deny network*)", binaries[0])
        listed = subprocess.run((*isolated, "--list"), cwd=state, env=env,
                                check=True, capture_output=True, text=True, timeout=30)
        tests = [line.removesuffix(": test") for line in listed.stdout.splitlines()
                 if line.endswith(f"::{CASE}: test")]
        if len(tests) != 1:
            raise ValueError("expected one retained-room test")
        subprocess.run((*isolated, "--exact", tests[0]), cwd=state, env=env, check=True, timeout=30)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=lambda path: Path(path).resolve(strict=True))
    parser.add_argument("target", type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    verify(args.compiler, args.target)
