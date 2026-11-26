#!/usr/bin/env python3
"""Replay the unified workspace with recovered release metadata and fixed vault bytes."""

import argparse
import json
from pathlib import Path
import runpy
import shutil
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


def from_bytes_test(work, original, fixed):
    harness = HERE / "fixed_bytes.rs"
    reference = work / "reference-from-bytes"
    (reference / "src").mkdir(parents=True)
    shutil.copyfile(harness, reference / "src/lib.rs")
    shutil.copyfile(original / "Cargo.lock", reference / "Cargo.lock")
    vault = json.dumps(str(original / "crates/marmish-vault"))
    (reference / "Cargo.toml").write_text(
        '[package]\nname = "observe-vault-from-bytes"\nversion = "0.0.0"\nedition = "2021"\n'
        '[dependencies]\nmarmish-vault = { path = ' + vault + ' }\n[workspace]\n')
    run("cargo", "+1.97.1", "metadata", "--offline", "--format-version", "1", cwd=reference)

    name = "from_bytes_owns_the_input_and_requires_drop"
    baseline = run("cargo", "+1.97.1", "test", "--locked", "--offline", "--release",
                   "--lib", "--", "--exact", name, cwd=reference)
    generated = work / "generated-from-bytes"
    fixed["link_test"](harness, work / "product", generated)
    for result in (baseline, run(generated, "--exact", name)):
        lines = result.decode().splitlines()
        assert lines.count(f"test {name} ... ok") == 1
        assert any(line.startswith("test result: ok. 1 passed; 0 failed; 0 ignored;") for line in lines)
    return {"name": name, "sourceSha256": sha(harness.read_bytes()), "originalAndGeneratedPassed": True}


def fixed_value_test(work, original, reference_target, *, predecessor_package=None):
    fixed = UNIFIED["lane"]("vault-fixed-bytes")
    name = fixed["NAME"]
    source = run("git", "-C", original, "show",
                 UNIFIED["ORIGINAL"] + ":crates/marmish-vault/src/lib.rs")
    marker = b"#[cfg(test)]\nmod tests {"
    _, body = source.split(marker)
    module = marker + body
    digest = sha(module)
    assert digest == fixed["MODULE"]
    fixed["passed"](run("cargo", "+1.97.1", "test", "--locked", "--offline", "--release",
                        "--target", "aarch64-apple-darwin", "--target-dir", reference_target,
                        "--lib", "--package", "marmish-vault", "--", "--exact", name,
                        cwd=original))
    harness = work / "strict-vault-values.rs"
    harness.write_bytes(b"pub use marmish_vault::*;\n" + module)
    executable = work / "probe-strict-vault-values"
    fixed["link_test"](harness, work / "product", executable)
    fixed["passed"](run(executable, "--exact", name))
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
    return {"name": name, "unchangedModuleSha256": digest, "originalAndGeneratedPassed": True,
            "standaloneReceiptSha256": sha(standalone_receipt),
            "fromBytesHoldout": from_bytes_test(work, original, fixed)}


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
