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
FRAMEWORK = "6e2d1ef00f286b6132b84e27a61805c659dfd437"
UNIFIED = runpy.run_path(str(HERE.parent / "unified-vault/verify.py"))
run = UNIFIED["run"]
sha = UNIFIED["sha"]


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
        tests = UNIFIED["vault_check"](work, "check-vault-fixed-bytes", args.original, work / "product",
                                      args.reference_target, "aarch64-apple-darwin", work)
        evidence = json.loads(run("cargo", "+1.97.1", "run", "--quiet", "--offline", "--locked",
                                  "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
                                  "--target-dir", work / "copy-test-tools", "--bin", "recovered-vault-evidence",
                                  "--", work / "inputs" / MANIFEST.name, tests["unchangedModuleSha256"]))
        receipt["recoveredBuildReport"] = evidence["recoveredBuildReport"]
        receipt["fixedVaultValue"] = dict(tests, standaloneReceiptSha256=evidence["standaloneReceiptSha256"])
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    args.receipt.with_name("workspace.lock").write_bytes(lock)


if __name__ == "__main__":
    main()
