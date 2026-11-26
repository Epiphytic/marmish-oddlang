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
FRAMEWORK = "595fb7a55dc2016cfab70c5b131bf2f6acfe9983"
UNIFIED = runpy.run_path(str(HERE.parent / "unified-vault/verify.py"))
run = UNIFIED["run"]
sha = UNIFIED["sha"]


def check_recovered_report(inputs):
    workspace = json.loads((inputs / MANIFEST.name).read_bytes())
    native_path = workspace["members"]["marmish-native"]["manifest"]["path"]
    native = json.loads((inputs / native_path).read_bytes())
    report, = [component for component in native["components"] if component["kind"] == "build-report"]
    prior = HERE.parent / "release-info-structure/darwin-arm64/receipt.json"
    evidence = json.loads(prior.read_bytes())["lanes"]["baseline"]
    digest = sha((inputs / report["graph"]["path"]).read_bytes())
    assert digest == report["graph"]["sha256"] == evidence["composedGraphSha256"]
    return {"graphSha256": digest, "recoveryReceiptSha256": sha(prior.read_bytes())}


def fixed_value_test(work, original, reference_target):
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
    standalone_receipt = HERE.parent / "vault-fixed-bytes/aarch64-apple-darwin.json"
    return {"name": name, "unchangedModuleSha256": digest, "originalAndGeneratedPassed": True,
            "standaloneReceiptSha256": sha(standalone_receipt.read_bytes())}


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
    system_temp = run("/usr/bin/getconf", "DARWIN_USER_TEMP_DIR").decode().strip()
    with tempfile.TemporaryDirectory(prefix="odd-unified-recovered-", dir=system_temp) as temp:
        work = Path(temp).resolve()
        receipt, lock = UNIFIED["qualify"](work, MANIFEST, args.original, args.framework,
                                          args.compiler_target / "debug/oddlang", args.reference_target)
        receipt["baseVerifierSha256"] = receipt["verifierSha256"]
        receipt["verifierSha256"] = sha(Path(__file__).read_bytes())
        receipt["recoveredBuildReport"] = check_recovered_report(work / "inputs")
        receipt["fixedVaultValue"] = fixed_value_test(work, args.original, args.reference_target)
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    args.receipt.with_name("workspace.lock").write_bytes(lock)


if __name__ == "__main__":
    main()
