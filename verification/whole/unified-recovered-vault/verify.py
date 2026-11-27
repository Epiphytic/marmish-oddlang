#!/usr/bin/env python3
"""Replay the unified workspace with recovered release metadata and fixed vault bytes."""

import argparse
import json
from pathlib import Path
import runpy
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / "workspace-v7-recovered-vault-engine.json"
VAULT_FRAMEWORK = "595fb7a55dc2016cfab70c5b131bf2f6acfe9983"
FRAMEWORK = "6e2d1ef00f286b6132b84e27a61805c659dfd437"
UNIFIED = runpy.run_path(str(HERE.parent / "unified-vault/verify.py"))
run = UNIFIED["run"]
sha = UNIFIED["sha"]


def check_recovered_report(inputs):
    workspace = json.loads((inputs / MANIFEST.name).read_bytes())
    native_path = workspace["members"]["marmish-native"]["manifest"]["path"]
    native = json.loads((inputs / native_path).read_bytes())
    report, = [component for component in native["components"] if component["kind"] == "build-report"]
    prior = HERE.parent / "release-info-structure/darwin-arm64/receipt.json"
    recovery = json.loads(prior.read_bytes())
    evidence = recovery["lanes"]["baseline"]
    assert recovery["representedRevision"] == evidence["sourceRevision"] == UNIFIED["ORIGINAL"]
    digest = sha((inputs / report["graph"]["path"]).read_bytes())
    assert digest == report["graph"]["sha256"] == evidence["composedGraphSha256"]
    return {"graphSha256": digest, "recoveryReceiptSha256": sha(prior.read_bytes())}


def fixed_value_test(work, original, reference_target, *, predecessor_package=None):
    tests = UNIFIED["vault_check"](work, "check-vault-fixed-bytes", original, work / "product",
                                  reference_target, "aarch64-apple-darwin", work)
    digest = tests["unchangedModuleSha256"]
    standalone_receipt = (HERE.parent / "vault-fixed-bytes/aarch64-apple-darwin.json").read_bytes()
    expected = json.loads(standalone_receipt)
    staged = work / "inputs"
    workspace = json.loads((staged / MANIFEST.name).read_bytes())
    reference = workspace["members"]["marmish-vault"]["manifest"]
    vault = json.loads((staged / reference["path"]).read_bytes())
    value, = [component for component in vault["components"] if "Secret" in component["reexports"]]
    assert expected["original"] == UNIFIED["ORIGINAL"] and expected["framework"] == VAULT_FRAMEWORK
    assert expected["unchangedModuleSha256"] == digest
    assert expected["graphSha256"] == value["graph"]["sha256"]
    package_sha = reference["sha256"]
    if predecessor_package is not None:
        baseline = json.loads(predecessor_package.read_bytes())
        inherited = baseline.pop("components")
        successor = dict(vault)
        components = successor.pop("components")
        if baseline != successor or len(components) <= len(inherited) or components[:len(inherited)] != inherited:
            raise ValueError("vault predecessor differs beyond appended components")
        package_sha = sha(predecessor_package.read_bytes())
    if expected["packageManifestSha256"] != package_sha:
        raise ValueError("fixed-value package differs from the exact native precedent")
    return dict(tests, standaloneReceiptSha256=sha(standalone_receipt))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "framework", "compiler_target", "reference_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    for checkout, commit in ((args.framework, FRAMEWORK), (args.original, UNIFIED["ORIGINAL"])):
        assert run("git", "-C", checkout, "rev-parse", "HEAD").decode().strip() == commit
        assert not run("git", "-C", checkout, "status", "--porcelain", "--untracked-files=no")
    assert not run("git", "-C", ROOT, "status", "--porcelain", "--untracked-files=all", "--",
                   "oddlang/portable", "verification/whole")
    run("cargo", "+1.97.1", "build", "--bin", "oddlang", "--offline", "--locked",
        "--target-dir", args.compiler_target, cwd=args.framework)
    run("cargo", "+1.97.1", "build", "--package", "marmish-native", "--bins", "--release",
        "--offline", "--locked", "--target", "aarch64-apple-darwin",
        "--target-dir", args.reference_target, cwd=args.original)
    pinned_lock = MANIFEST.with_suffix("") / "workspace.lock"
    predecessor_lock = PORTABLE / "workspace-v7-vault-engine/workspace.lock"
    before = json.loads(predecessor_lock.read_bytes())["members"]
    selected = json.loads(pinned_lock.read_bytes())["members"]
    unchanged = ["marmish-core", "marmish-engine", "marmish-stream"]
    assert all(selected[name] == before[name] for name in unchanged)
    system_temp = run("/usr/bin/getconf", "DARWIN_USER_TEMP_DIR").decode().strip()
    with tempfile.TemporaryDirectory(prefix="odd-unified-recovered-", dir=system_temp) as temp:
        work = Path(temp).resolve()
        receipt, lock = UNIFIED["qualify"](work, MANIFEST, args.original, args.framework,
                                          args.compiler_target / "debug/oddlang", args.reference_target)
        receipt["baseVerifierSha256"] = receipt["verifierSha256"]
        receipt["verifierSha256"] = sha(Path(__file__).read_bytes())
        assert lock == pinned_lock.read_bytes()
        receipt["unchangedMemberLocks"] = unchanged
        receipt["recoveredBuildReport"] = check_recovered_report(work / "inputs")
        receipt["fixedVaultValue"] = fixed_value_test(work, args.original, args.reference_target)
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    args.receipt.with_name("workspace.lock").write_bytes(lock)


if __name__ == "__main__":
    main()
