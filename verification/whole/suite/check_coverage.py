"""Inspect a stored candidate receipt against its named upstream release catalog."""
import argparse
import json
import re
from pathlib import Path
import sys

from candidate_coverage import CORE, STREAM, NATIVE, CANDIDATES, HERE, PIN, reconcile, sha


import selected_tests


def inspect(receipt):
    host = receipt["target"]
    if host not in {"aarch64-apple-darwin","aarch64-unknown-linux-gnu"}:
        raise ValueError("unknown native target")
    data = (HERE / f"named-release-{host}.json").read_bytes()
    catalog = json.loads(data)
    matches = [(key,manifest) for key,(manifest,revision) in CANDIDATES.items() if revision == receipt["framework"] and sha(manifest.read_bytes()) == receipt["manifestSha256"]]
    if len(matches) != 1:
        raise ValueError("unknown workspace or compiler revision")
    candidate, manifest = matches[0]
    expected_pins = dict(original=PIN, profile=catalog["selection"],
                         inventorySha256=sha(data), manifestSha256=sha(manifest.read_bytes()),
                         workspaceLockSha256=sha((manifest.with_suffix("")/"workspace.lock").read_bytes()))
    if catalog["upstream"] != PIN or any(receipt[k] != value for k,value in expected_pins.items()):
        raise ValueError("candidate or upstream profile seal differs")
    rows = receipt["harnesses"]
    if set(rows) != set(catalog["harnesses"]):
        raise ValueError("missing or unexpected harness")
    witnesses = {k:v["passed"] for k,v in rows.items() if v["passed"]}
    native = receipt["nativeProcessTest"]
    reader = candidate == "seven"
    if reader:
        expected_members = set(json.loads((manifest.with_suffix("")/"workspace.lock").read_bytes())["members"])
        built = receipt["builtLibraries"]
        if set(built) != expected_members or not all(re.fullmatch(r"[0-9a-f]{64}",d) for d in built.values()):
            raise ValueError("missing or invalid native workspace library artifacts")
    elif "builtLibraries" in receipt:
        raise ValueError("unexpected workspace library evidence")
    partial = selected_tests.cases(host,reader) if candidate in {"recovered","seven"} else {}
    expected_sources = dict(selected_tests.EXPECTED)
    if reader:
        expected_sources["prototypeReader"] = selected_tests.READER_CASE["TEST_HASH"]
    expected_processes = {k:{name:2 if k == selected_tests.ENGINE else 1 for name in names}
                          for k,names in partial.items()}
    if receipt.get("passingProcesses") != (expected_processes if partial else None):
        raise ValueError("partial test process evidence differs")
    if any(witnesses.get(k) != v for k,v in partial.items()) or receipt.get("selectedTestSources") != (expected_sources if partial else None):
        raise ValueError("partial harness source or observed cases disagree")
    if set(witnesses) != {CORE, STREAM, NATIVE} | set(partial) or witnesses[NATIVE] != [native["test"]]:
        raise ValueError("pass without matching qualified harness")
    for harness in (CORE, STREAM, NATIVE):
        if witnesses[harness] != sorted(catalog["harnesses"][harness]["tests"]):
            raise ValueError(f"incomplete qualified harness: {harness}")
    prior = json.loads((HERE.parent / "workspace" / f"v7-{host}.json").read_bytes())
    reference = prior["nativeProcessLimits"]
    if prior["original"] != PIN or receipt["upstreamModules"] != {k:v for k,v in prior["testModules"].items() if k != "exactCopyFrames"}:
        raise ValueError("injected test modules differ from the pinned upstream suites")
    if any(native[k] != reference[k] for k in ("source", "source_sha256", "test", "upstream_commit", "compiler")):
        raise ValueError("process test source or compiler differs")
    if native["executables"] != receipt["releaseBinarySha256"] or set(native["executables"]) != {"marmish", "marmishd"}:
        raise ValueError("missing generated process role")
    if native["staged_pair_exit"] != 0 or set(native["mutants"]) != {"marmish","marmishd"} or any(
        case["exit"] != 101 or case["failed_assertion"] != "!result.stdout.is_empty()" or case["sha256"] != sha(b"#!/bin/sh\nexit 0\n") for case in native["mutants"].values()
    ):
        raise ValueError("native executable witness failed its control")
    canonical, totals = reconcile(catalog, witnesses)
    if rows != canonical or receipt["totals"] != totals:
        raise ValueError("case dispositions or totals disagree with the catalog")
    return totals


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt",type=Path)
    parser.add_argument("--require-complete",action="store_true")
    args = parser.parse_args()
    totals = inspect(json.loads(args.receipt.read_bytes()))
    print(json.dumps(totals, sort_keys=True))
    if args.require_complete and totals["unverified"]:
        sys.exit(1)
