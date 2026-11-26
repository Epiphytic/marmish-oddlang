#!/usr/bin/env python3
"""Measure the original public stream corpus on a native Linux Rust host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from framework_pin import REVISION, verify_checkout

PIN = "af54339d250290490d62d4475a77539b7e739071"
LOCK = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3"
EXPECTED = ("publicObservationSha256", "priorCases", "authoredCases", "seededCases",
            "mixedBinaryAndJson", "negativeControls", "freezeSha256")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("Linux host required")
    triple = {"aarch64": "aarch64-unknown-linux-gnu", "x86_64": "x86_64-unknown-linux-gnu"}[platform.machine()]
    args.out.mkdir(exist_ok=True, parents=True)
    verify_checkout(args.framework)
    oracle = args.framework / "examples/marmish/portable-oracle"
    expected = json.loads((oracle / "original-receipt.json").read_bytes())
    verified = {}
    for profile in ("dev", "release"):
        report = args.out / (profile + ".json")
        subprocess.run([sys.executable, "-B", oracle / "run.py", "--upstream", args.original.resolve(),
                        "--original-target", triple, "--original-revision", PIN, "--original-lock", LOCK,
                        "--original-profile", profile, "--receipt", report], check=True,
                       env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"))
        result = json.loads(report.read_bytes())
        for key in EXPECTED:
            if result[key] != expected[key]:
                sys.exit(f"{profile} differs from independent baseline for {key}")
        verified[profile] = hashlib.sha256(report.read_bytes()).hexdigest()
    verify_checkout(args.framework)
    identity = {"frameworkRevision": REVISION, "rustHost": triple, "kernel": platform.release(), "libc": list(platform.libc_ver()),
                "os": platform.freedesktop_os_release()["PRETTY_NAME"], "receipts": verified,
                "oracleSha256": hashlib.sha256((oracle / "run.py").read_bytes()).hexdigest()}
    (args.out / "host.json").write_text(json.dumps(identity, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
