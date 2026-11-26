#!/usr/bin/env python3
"""Repeat the pinned public stream and Data qualification on the recorded host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from framework_pin import REVISION as FRAMEWORK_REVISION, build_cli

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


def compare_origin(current, archived):
    for key in ("buildLocale", "cargoLockSha256", "compiler", "features", "profile", "revision", "sourceSha256", "target"):
        require(current[key] == archived[key], f"original build differs at {key}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=Path)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--holdout", type=Path, help="optional independently held sealed capsule")
    args = parser.parse_args()
    with build_cli(args.framework.resolve()) as oddlang:
        replay(args, oddlang)


def replay(args, oddlang):
    oracle = args.framework.resolve() / "examples/marmish/portable-oracle"
    contract = oracle.parent / "portable-contract"
    upstream = args.upstream.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    native = run("rustc", "+1.97.1", "-vV")
    triple, = [line.removeprefix("host: ") for line in native.splitlines() if line.startswith("host: ")]
    pin = read(RELEASE / "release.json")["graphSha256"]
    baseline = read(oracle / "original-receipt.json")
    old_data = read(oracle / "tagged-qualification.json")
    require(run("git", "-C", upstream, "rev-parse", "HEAD") == REVISION, "original revision mismatch")
    for path in ("Cargo.lock", "crates/marmish-stream"):
        prior = run("git", "-C", upstream, "rev-parse", f"{baseline['provenance']['revision']}:{path}")
        require(prior == run("git", "-C", upstream, "rev-parse", f"{REVISION}:{path}"), f"corpus input changed: {path}")
    (output / "framework.json").write_text(json.dumps({"sourceRevision": FRAMEWORK_REVISION,
        "cleanCheckout": True, "freshBuildSha256": hashlib.sha256(oddlang.read_bytes()).hexdigest()}, indent=2) + "\n")
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
            archived = read(HERE / receipt.name)
            for key in archived.keys() - {"provenance", "holdout"}:
                require(current.get(key) == archived[key], f"archived {profile} differs at {key}")
            compare_origin(current["provenance"], archived["provenance"])
            if args.holdout:
                require(current["holdout"] == archived["holdout"], f"{profile} sealed receipt differs")
    original_archive = hashlib.sha256((HERE / "public.json.gz").read_bytes()).hexdigest()
    require(hashlib.sha256((output / "public.json.gz").read_bytes()).hexdigest() == original_archive,
            "public observation archive mismatch")
    if triple == "aarch64-apple-darwin":
        receipt = output / "data-release.json"
        print(run(sys.executable, oracle / "qualify_tagged.py", *arguments, "--oddlang", oddlang,
                  "--graph", RELEASE / "contract.ttl", "--hints", RELEASE / "hints.json",
                  "--original-profile", "release", "--receipt", receipt), flush=True)
        current = read(receipt)
        for key in ("approvedGraphSha256", "publicCaseIds", "mixedPeers", "mutation", "comparedOperationResults"):
            require(current[key] == old_data[key], f"Data qualification changed {key}")
        archived = read(HERE / receipt.name)
        for key in archived.keys() - {"build", "buildWithoutHints", "mutatedBuild", "originalCaller", "sealedCases"}:
            require(current.get(key) == archived[key], f"archived Data differs at {key}")
        compare_origin(current["originalCaller"], archived["originalCaller"])
        for build in ("build", "buildWithoutHints", "mutatedBuild"):
            for key in ("generatedSourceSha256", "graphFileSha256", "graphSha256", "projectionLockSha256"):
                require(current[build][key] == archived[build][key], f"archived {build} differs at {key}")
        if args.holdout:
            require(current["sealedCases"] == archived["sealedCases"], "archived Data sealed coverage differs")
            require(current["sealedCases"] == old_data["sealedCases"], "Data sealed coverage changed")
    else:
        print("Data target isolation uses the macOS qualification sandbox. This run covers the native stream oracle.")


if __name__ == "__main__":
    main()
