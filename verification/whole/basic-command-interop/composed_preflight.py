#!/usr/bin/env python3
"""Check generated setup and provider entry points with networking denied."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from build import PORTABLE, TOOLS, sha

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


COMPOSITION = (
    "workspace-composed-basic-owner.json",
    "marmish-native-composed-basic-owner-package.json",
    "composed-basic-owner-v1/marmish-cli.json",
)


def pinned_json(path, expected):
    if path.is_symlink():
        raise ValueError("receipt must be an ordinary file")
    body = path.read_bytes()
    if hashlib.sha256(body).hexdigest() != expected:
        raise ValueError("receipt digest differs")
    return json.loads(body)


def load_candidate(export, expected):
    build = pinned_json(export / "receipt.json", expected)
    if build["format"] != "marmish-basic-command-generated-build-v1":
        raise ValueError("incorrect build receipt format")
    if (not build["readIsolation"].startswith("Host Seatbelt default deny")
            or not {"originalSource", "originalBinary", "oracleFixture", "defaultDenyCanary"}
                   <= build["deniedReadProbes"].keys()):
        raise ValueError("candidate was not built with the required source exclusions")
    for relative in COMPOSITION:
        if build["stagedContracts"].get(relative) != sha(PORTABLE / relative):
            raise ValueError(f"staged composition pin differs: {relative}")
    roles = build["cliSurfaces"]["marmish"]["boundOperations"]
    if not {"setup", "*"} <= set(roles) or set(build["artifacts"]) != {"marmish", "marmishd"}:
        raise ValueError("composed role inventory differs")
    for name, record in build["artifacts"].items():
        binary = export / name
        if binary.is_symlink() or sha(binary) != record["sha256"]:
            raise ValueError(f"candidate executable digest differs: {name}")
    return build


def observe(export, expected):
    load_candidate(export, expected)
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
