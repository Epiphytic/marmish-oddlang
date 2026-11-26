#!/usr/bin/env python3
"""Check generated setup and provider entry points with networking denied."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from build import TOOLS, read, sha

CASES = {
    "setupWithoutFileProvider": (
        "marmish", ["setup", "--role", "client", "--no-grants"], 1,
        b"marmish: profile preparation requires the file key provider\n",
    ),
    "foregroundWithoutState": (
        "marmishd", ["--foreground"], 125,
        b"marmish: foreground status owner requires explicit state and a prepared file profile\n",
    ),
}


def observe(export, expected):
    receipt = export / "receipt.json"
    if receipt.is_symlink() or sha(receipt) != expected:
        raise ValueError("candidate build receipt digest differs")
    build = read(receipt)
    if (build["format"] != "marmish-basic-command-generated-build-v1"
            or "workspace-composed-basic-owner.json" not in build["stagedContracts"]):
        raise ValueError("incorrect composed candidate descriptor")
    if ("setup" not in build["cliSurfaces"]["marmish"]["boundOperations"]
            or set(build["artifacts"]) != {"marmish", "marmishd"}):
        raise ValueError("composed role inventory differs")
    for name, record in build["artifacts"].items():
        binary = export / name
        if binary.is_symlink() or sha(binary) != record["sha256"]:
            raise ValueError(f"candidate executable digest differs: {name}")
    results = {}
    with tempfile.TemporaryDirectory(prefix="odd-composed-preflight-") as directory:
        scratch = Path(directory).resolve()
        launch = TOOLS.isolated(scratch, (export,))
        environment = {"HOME": str(scratch), "TMPDIR": str(scratch), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
        for case, (binary, arguments, code, error) in CASES.items():
            result = subprocess.run([*launch, export / binary, *arguments], cwd=scratch,
                                    input=b"", capture_output=True, timeout=30, env=environment)
            if (result.returncode, result.stdout, result.stderr) != (code, b"", error):
                raise ValueError(f"unexpected composed candidate preflight: {case}")
            if any(scratch.iterdir()):
                raise ValueError(f"unprepared candidate created state: {case}")
            results[case] = {"role": binary, "argv": arguments, "exit": code,
                             "stdout": "", "stderr": error.decode()}
    return {
        "format": "marmish-composed-owner-preflight-v1",
        "buildReceiptSha256": expected,
        "preflightSha256": sha(Path(__file__)),
        "cases": results,
        "workspaceRemoved": not scratch.exists(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    args = parser.parse_args()
    with args.receipt.open("x") as result:
        result.write(json.dumps(observe(args.export, args.build_receipt_sha256), indent=2) + "\n")


if __name__ == "__main__":
    main()
