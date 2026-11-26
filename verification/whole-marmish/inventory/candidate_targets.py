"""Compare a checked native suite receipt's built targets with pinned upstream metadata."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "whole/suite"))
from check_coverage import inspect
from candidate_coverage import CANDIDATES


def compare(receipt):
    inspect(receipt)
    if "builtLibraries" not in receipt:
        raise ValueError("selected receipt has no library build evidence")
    original = json.loads((HERE / "inventory.json").read_bytes())
    if original["commit"] != receipt["original"]:
        raise ValueError("upstream target inventory revision differs from suite")
    root, = (w for w in original["workspaces"] if w["manifest"] == "Cargo.toml")
    expected = {(package["name"], target["kind"][0], target["name"]): target["required_features"]
                for package in root["packages"] for target in package["targets"]
                if target["kind"] in (["bin"], ["lib"])}
    candidate, = (entry.manifest for entry in CANDIDATES.values()
                  if entry.compiler == receipt["framework"]
                  and sha256(entry.manifest.read_bytes()).hexdigest() == receipt["manifestSha256"])
    binaries = {}
    observed = set()
    declared = set()
    for name, member in json.loads(candidate.read_bytes())["members"].items():
        pin = member["manifest"]
        raw = (candidate.parent / pin["path"]).read_bytes()
        package = json.loads(raw)
        if sha256(raw).hexdigest() != pin["sha256"] or package["name"] != name:
            raise ValueError(f"mismatched package manifest for {name}")
        library = (name, "lib", name.replace("-", "_"))
        declared.add(library)
        if name in receipt["builtLibraries"]:
            observed.add(library)
        for binary in package.get("binaries", []):
            spelling = binary["name"]
            if spelling in binaries:
                raise ValueError(f"ambiguous binary owner: {spelling}")
            binaries[spelling] = name
            declared.add((name, "bin", spelling))
    for spelling in receipt["releaseBinarySha256"]:
        observed.add((binaries[spelling], "bin", spelling))
    return {"upstream": original["commit"], "host": receipt["target"],
            "workspaceLockSha256": receipt["workspaceLockSha256"],
            "targets": [{"package": p, "kind": k, "name": n, "requiredFeatures": features,
                         "declared": (p, k, n) in declared, "buildObserved": (p, k, n) in observed}
                        for (p,k,n), features in sorted(expected.items())],
            "extraObserved": sorted(observed - expected.keys())}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("receipt", type=Path, help="native suite receipt including builtLibraries")
    args = cli.parse_args()
    print(json.dumps(compare(json.loads(args.receipt.read_bytes())), indent=2))
