#!/usr/bin/env python3
"""Observe unavailable operations on digest-bound candidate binaries without peers."""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from build import TOOLS, read, sha


UNAVAILABLE = b"command operation unavailable\n"
PROFILE_REQUIRED = b"marmish: foreground status owner requires explicit state and a prepared file profile\n"
CASES = {
    "foregroundSubmit": ("marmish", ["audit-peer", "/usr/bin/true"], UNAVAILABLE),
    "detachedSubmit": ("marmish", ["--detach", "audit-peer", "/usr/bin/true"], UNAVAILABLE),
    "wait": ("marmish", ["wait", "--latest", "audit-peer"], UNAVAILABLE),
    "setup": ("marmish", ["setup", "--role", "client", "--no-grants"], UNAVAILABLE),
    "authorize": ("marmish", ["keychain-authorize"], UNAVAILABLE),
    "daemonWithoutState": ("marmishd", ["--foreground"], PROFILE_REQUIRED),
}


def observe(export, expected_receipt):
    receipt = export / "receipt.json"
    if receipt.is_symlink() or sha(receipt) != expected_receipt:
        raise ValueError("generated build receipt digest mismatch")
    build = read(receipt)
    if build.get("format") != "marmish-basic-command-generated-build-v1":
        raise ValueError("unexpected generated build receipt format")
    for role in {role for role, _, _ in CASES.values()}:
        executable = export / role
        if executable.is_symlink() or sha(executable) != build["artifacts"][role]["sha256"]:
            raise ValueError(f"generated executable digest mismatch: {role}")
    results = {}
    with tempfile.TemporaryDirectory(prefix="odd-basic-candidate-check-") as directory:
        work = Path(directory).resolve()
        launch = TOOLS.isolated(work, (export,))
        for name, (role, argv, stderr) in CASES.items():
            executable = export / role
            scratch = work / name
            scratch.mkdir()
            actual = subprocess.run(
                [*launch, executable, *argv], cwd=scratch, input=b"", capture_output=True,
                timeout=30, env={"HOME": str(scratch), "TMPDIR": str(scratch),
                                 "PATH": "/usr/bin:/bin", "LC_ALL": "C"},
            )
            if (actual.returncode, actual.stdout, actual.stderr) != (125, b"", stderr):
                raise ValueError(f"unexpected operation availability: {name}")
            if any(scratch.iterdir()):
                raise ValueError(f"operation wrote to its private workspace: {name}")
            results[name] = {
                "role": role, "argv": argv, "exit": actual.returncode,
                "stdout": actual.stdout.decode("ascii"), "stderr": actual.stderr.decode("ascii"),
                "workspaceUnchanged": True,
            }
    return {"format": "marmish-basic-command-generated-preflight-v1",
            "buildReceiptSha256": expected_receipt, "preflightSha256": sha(Path(__file__)),
            "cases": results, "workspaceRemoved": not work.exists()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("output", type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    args = parser.parse_args()
    result = observe(args.export, args.build_receipt_sha256)
    with args.output.open("x") as receipt:
        receipt.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
