#!/usr/bin/env python3
"""Check the pinned room candidate's unprepared entry points without networking."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from build import PORTABLE, TOOLS, sha
from composed_preflight import CASES, pinned_json

INPUTS = (
    "marmish-native-bounded-owner-prepare-package.json",
    "bounded-owner-prepare-v1/foreground.json",
    "bounded-owner-prepare-v1/status.rust.json",
)


def observe(export, expected, workspace):
    build = pinned_json(export / "receipt.json", expected)
    if (build["format"] != "marmish-basic-command-generated-build-v1"
            or build["platform"] != "aarch64-apple-darwin"
            or not {"originalSource", "originalBinary", "oracleFixture", "defaultDenyCanary"}
                   <= build["deniedReadProbes"].keys()
            or set(build["artifacts"]) != {"marmish", "marmishd"}):
        raise ValueError("unexpected build provenance")
    selected = {}
    for relative in (workspace, *INPUTS):
        actual = sha(PORTABLE / relative)
        if build["stagedContracts"].get(relative) != actual:
            raise ValueError("incorrect owner prepare composition pin")
        selected[relative] = actual
    for name, record in build["artifacts"].items():
        binary = export / name
        if binary.is_symlink() or sha(binary) != record["sha256"]:
            raise ValueError("unexpected executable")
    cases = {}
    with tempfile.TemporaryDirectory(prefix="odd-prepare-preflight-") as directory:
        scratch = Path(directory).resolve()
        launch = TOOLS.isolated(scratch, (export,))
        env = {"HOME": str(scratch), "TMPDIR": str(scratch), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
        for case, (name, args, code, error) in CASES.items():
            child = subprocess.run([*launch, export / name, *args], cwd=scratch, env=env,
                                   input=b"", capture_output=True, timeout=30, check=False)
            if (child.returncode, child.stdout, child.stderr) != (code, b"", error) or any(scratch.iterdir()):
                raise ValueError("unprepared candidate boundaries differ")
            cases[case] = {"role": name, "argv": args, "exit": code, "stdout": "", "stderr": error.decode()}
    return {"format": "marmish-bounded-room-prepare-preflight-v1", "buildReceiptSha256": expected,
            "preflightSha256": sha(Path(__file__)), "cases": cases, "compositionPins": selected,
            "workspaceRemoved": not scratch.exists()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=lambda path: Path(path).resolve(strict=True))
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    parser.add_argument("--workspace", choices=("workspace-bounded-owner-prepare.json",
                                                  "workspace-bounded-owner-prepare-fixed.json"),
                        default="workspace-bounded-owner-prepare.json")
    options = parser.parse_args()
    result = observe(options.export, options.build_receipt_sha256, options.workspace)
    with options.receipt.open("x") as output:
        output.write(json.dumps(result, indent=2) + "\n")
