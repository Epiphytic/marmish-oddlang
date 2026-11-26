#!/usr/bin/env python3
"""Repeat the pinned public stream and Data qualification on the recorded host."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
RELEASE = HERE.parents[2] / "oddlang/portable/copy-data-v1"
REVISION = "af54339d250290490d62d4475a77539b7e739071"
LOCK = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3"


def run(*arguments):
    return subprocess.check_output(arguments, text=True, env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1")).strip()


def read(path):
    return json.loads(path.read_bytes())


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=Path)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--holdout", type=Path, help="optional independently held sealed capsule")
    args = parser.parse_args()
    oracle = args.framework.resolve() / "examples/marmish/portable-oracle"
    contract = oracle.parent / "portable-contract"
    upstream = args.upstream.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    oddlang = args.oddlang.resolve()
    native = run("rustc", "+1.97.1", "-vV")
    triple, = [line.removeprefix("host: ") for line in native.splitlines() if line.startswith("host: ")]
    pin = read(RELEASE / "release.json")["graphSha256"]
    baseline = read(oracle / "original-receipt.json")
    old_data = read(oracle / "tagged-qualification.json")
    require(run("git", "-C", upstream, "rev-parse", "HEAD") == REVISION, "original revision mismatch")
    with tempfile.TemporaryDirectory() as temp:
        recovered = Path(temp) / "data.ttl"
        run(oddlang, "recover", "tagged-contract", upstream, contract / "selection.json", contract / "data-overlay.ttl",
            "urn:marmish:portable:copy-frame:v1:Data", recovered)
        require(run(oddlang, "tagged-wire", "digest", recovered) == pin, "recovered different graph")
        require(run(oddlang, "tagged-wire", "digest", RELEASE / "contract.ttl") == pin, "published graph differs")

    arguments = ["--upstream", upstream, "--original-target", triple, "--original-revision", REVISION,
                 "--original-lock", LOCK]
    if args.holdout:
        arguments.extend(["--holdout", args.holdout.resolve()])
    for profile in ("dev", "release"):
        receipt = output / f"original-{profile}.json"
        archive = ["--observations", output / "public.json.gz"] if profile == "dev" else []
        print(run(sys.executable, oracle / "run.py", *arguments, "--original-profile", profile,
                  "--receipt", receipt, *archive), flush=True)
        current = read(receipt)
        for key in ("publicObservationSha256", "priorCases", "authoredCases", "seededCases", "mixedBinaryAndJson",
                    "negativeControls", "freezeSha256"):
            require(current[key] == baseline[key], f"{profile} changed {key}")
    if triple == "aarch64-apple-darwin":
        receipt = output / "data-release.json"
        print(run(sys.executable, oracle / "qualify_tagged.py", *arguments, "--oddlang", oddlang,
                  "--graph", RELEASE / "contract.ttl", "--hints", RELEASE / "hints.json",
                  "--original-profile", "release", "--receipt", receipt), flush=True)
        current = read(receipt)
        for key in ("approvedGraphSha256", "publicCaseIds", "mixedPeers", "mutation", "comparedOperationResults"):
            require(current[key] == old_data[key], f"Data qualification changed {key}")
        if args.holdout:
            require(current["sealedCases"] == old_data["sealedCases"], "Data sealed coverage changed")
    else:
        print("Data target isolation uses the macOS qualification sandbox. This run covers the native stream oracle.")


if __name__ == "__main__":
    main()
