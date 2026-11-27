#!/usr/bin/env python3
"""Compare a generated typed status program with native original IPC observations."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from qualify import guard_checks, parser_counts

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
SHAPE = PORTABLE / "native-status-typed-v1"
PACKAGE = PORTABLE / "native-status-typed-package.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original_bins", type=Path)
    parser.add_argument("generated_bins", type=Path)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    candidate = (args.generated_bins / "marmish").resolve()
    original = (args.original_bins / "marmish").resolve()
    reference = json.loads((HERE / "macos-arm64.json").read_bytes())
    if reference["mode"] != "original" or any(
        digest(args.original_bins / name) != reference["originalBinaries"][name]
        for name in ("marmish", "marmishd")
    ):
        raise ValueError("original native executables differ from the macOS baseline")
    with tempfile.TemporaryDirectory(prefix="typed-status-evidence-") as directory:
        work = Path(directory)
        mixed, extended, nested = (work / name for name in ("mixed.json", "extended.json", "nested.json"))
        for script, command in (
            ("observe.py", [args.original_bins, mixed, "--candidate", candidate]),
            ("reply_shapes.py", [candidate, extended, "--compare", HERE / "reply-shapes-macos-arm64.json"]),
            ("typed_reply_variants.py", [candidate, nested, "--compare", HERE / "typed-reply-variants-macos-arm64.json"]),
        ):
            subprocess.run([sys.executable, "-B", HERE / script, *command], check=True, timeout=90)
        evidence = json.loads(mixed.read_bytes())
        extended_rows = json.loads(extended.read_bytes())
        nested_rows = json.loads(nested.read_bytes())
    if {evidence["candidateSha256"], extended_rows["binarySha256"], nested_rows["binarySha256"]} != {digest(candidate)}:
        raise ValueError("observations use different binaries")
    guards, uncovered = guard_checks(original, candidate)
    evidence.update(
        frameworkRevision=args.framework_revision,
        graphSha256=digest(SHAPE / "graph.json"),
        packageManifestSha256=digest(PACKAGE),
        packageLockSha256=digest(SHAPE / "package.lock"),
        replyObligationsSha256=digest(SHAPE / "reply-obligations.json"),
        parserObservations=parser_counts(args.generated_bins),
        orderedGuardsCompared=guards,
        uncoveredLanes=uncovered,
        extendedReplies=extended_rows,
        nestedReplies=nested_rows,
        qualificationScope="native client with explicit state and socket arguments",
    )
    args.receipt.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
