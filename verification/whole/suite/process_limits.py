#!/usr/bin/env python3
"""Run the unchanged upstream native help test against explicitly staged binaries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from upstream import PIN, git

SOURCE = "crates/marmish-native/tests/process_limits.rs"
CASE = "actual_native_entrypoints_accept_inherited_limits"


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(upstream, rustc, libc, binaries, cargo_harness):
    if git(upstream, "rev-parse", "HEAD").decode().strip() != PIN:
        raise ValueError(f"expected upstream HEAD {PIN}")

    with tempfile.TemporaryDirectory(prefix="marmish-suite-") as directory:
        root = Path(directory).resolve()
        source = root / "process_limits.rs"
        source.write_bytes(git(upstream, "show", f"{PIN}:{SOURCE}"))
        baseline_env = dict(os.environ)
        staged = {}
        for name, path in binaries.items():
            staged[name] = root / name
            shutil.copy2(path, staged[name])
            baseline_env[f"CARGO_BIN_EXE_{name}"] = str(staged[name])

        executable = root / "process_limits"
        subprocess.run(
            [rustc, "--edition=2024", "--test", str(source),
             "--extern", f"libc={libc}", "-o", str(executable)],
            cwd=root, env=baseline_env, check=True, timeout=120,
        )

        def execute(test, env):
            outcome = subprocess.run(
                [str(test), "--exact", CASE], cwd=root, env=env,
                capture_output=True, text=True, timeout=30,
            )
            if "running 1 test" not in outcome.stdout:
                raise RuntimeError("the harness must select exactly one upstream test")
            return outcome

        good = execute(executable, baseline_env)
        if good.returncode:
            raise RuntimeError(good.stdout + good.stderr)
        receipt = {
            "upstream_commit": PIN,
            "source": SOURCE,
            "test": CASE,
            "source_sha256": sha256(source),
            "compiler": subprocess.check_output([rustc, "--version"], text=True).strip(),
            "libc_sha256": sha256(libc),
            "executables": {name: sha256(path) for name, path in staged.items()},
            "staged_pair_exit": good.returncode,
            "mutants": {},
        }
        for name, path in staged.items():
            # Exit normally with empty stdout and stderr. Replace just one role at a time.
            path.unlink()
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o755)
            bad = execute(executable, baseline_env)
            if bad.returncode != 101 or "assertion failed: !result.stdout.is_empty()" not in bad.stdout:
                raise RuntimeError(bad.stdout + bad.stderr)
            receipt["mutants"][name] = {
                "sha256": sha256(path),
                "exit": bad.returncode,
                "failed_assertion": "!result.stdout.is_empty()",
            }
            path.unlink()
            shutil.copy2(binaries[name], path)

        if cargo_harness is not None:
            # Runtime overrides point to missing files. Cargo's original harness
            # can still pass if it runs the binaries baked in by env!().
            runtime_env = dict(baseline_env)
            for name in staged:
                runtime_env[f"CARGO_BIN_EXE_{name}"] = str(root / "missing")
            original = execute(cargo_harness, runtime_env)
            if original.returncode:
                raise RuntimeError(original.stdout + original.stderr)
            receipt["cargo_harness_with_missing_runtime_overrides"] = {
                "harness_sha256": sha256(cargo_harness),
                "exit": original.returncode,
            }
        return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--rustc", required=True, type=Path)
    parser.add_argument("--libc", required=True, type=Path)
    parser.add_argument("--client", required=True, type=Path)
    parser.add_argument("--daemon", required=True, type=Path)
    parser.add_argument("--cargo-harness", type=Path)
    args = parser.parse_args()
    receipt = run(
        args.upstream.resolve(), args.rustc.resolve(), args.libc.resolve(),
        {"marmish": args.client.resolve(), "marmishd": args.daemon.resolve()},
        args.cargo_harness.resolve() if args.cargo_harness else None,
    )
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
