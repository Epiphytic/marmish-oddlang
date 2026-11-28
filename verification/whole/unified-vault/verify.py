#!/usr/bin/env python3
"""Build a sealed native workspace and replay its offline library contracts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK = "7ed1c29acde2cc4cfdc58c2489935e703470ae70"


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def lane(name, file="verify.py"):
    return runpy.run_path(str(HERE.parent / name / file))


def sandbox(work, original, compiler, framework, reference_target, *, reference_executable=None):
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    protected = [ROOT.resolve(), original.resolve(), framework.resolve(), reference_target.resolve(),
                 Path("/private/tmp"), Path.home() / "repos"]
    assert not any(work.is_relative_to(path) for path in protected), work
    profile = "(version 1)(allow default)(deny network*)"
    profile += "".join("(deny file-read* (subpath " + json.dumps(str(path)) + "))"
                       for path in protected)
    prefix = ["/usr/bin/sandbox-exec", "-p", profile]
    with tempfile.TemporaryDirectory(prefix="odd-source-probe-", dir="/private/tmp") as outside:
        sentinel = Path(outside) / "Cargo.toml"
        sentinel.write_text("[workspace]\n")
        if reference_executable is None:
            reference_executable = reference_target / "aarch64-apple-darwin/release/marmish"
        assert reference_executable.resolve().is_relative_to(reference_target.resolve())
        probes = {
            "companionCaller": HERE.parent / "vault-values/caller.rs",
            "originalLibrary": original / "crates/marmish-vault/src/lib.rs",
            "frameworkLibrary": framework / "src/lib.rs",
            "referenceExecutable": reference_executable,
            "externalCheckoutExample": sentinel,
        }
        for file in probes.values():
            assert file.is_file(), file
            rejected = subprocess.run([*prefix, "/bin/cat", file], capture_output=True)
            assert rejected.returncode and b"Operation not permitted" in rejected.stderr, file
    cli = work / "oddlang"
    shutil.copyfile(compiler, cli)
    cli.chmod(0o755)
    return [*prefix, "/usr/bin/env", f"TMPDIR={work}"], cli, sorted(probes)


def vault_check(work, binary, *args):
    return json.loads(run("cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline",
                          "--manifest-path", HERE.parent / "copy-manifest/Cargo.toml", "--bin", binary,
                          "--target-dir", work / "copy-test-tools", "--", "workspace", *args))


def copy_tests(work, original, product):
    manifest = HERE.parent / "copy-manifest/Cargo.toml"
    target = work / "copy-test-tools"
    run("cargo", "+1.97.1", "build", "--quiet", "--locked", "--offline",
        "--manifest-path", manifest, "--bin", "check-copy-frame", "--target-dir", target)
    check = target / "debug/check-copy-frame"
    reference = json.loads(run(check, "test-package", original, original / "crates/marmish-stream",
                               original / "Cargo.lock", work / "original-copy"))
    generated = json.loads(run(check, "test-package", original, product / "marmish-stream",
                               product / "Cargo.lock", work / "generated-copy"))
    if reference != generated:
        raise ValueError("original and generated copy test suites differ")
    return generated


def native_error_observations(work, original, product, reference_target):
    error_tool = work / "copy-test-tools/debug/native-error-replay"
    run("cargo", "+1.97.1", "build", "--quiet", "--offline", "--locked",
        "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
        "--bin", "native-error-replay", "--target-dir", work / "copy-test-tools")
    old_errors = run(error_tool, "probe", original, reference_target, work / "original-errors")
    new_errors = run(error_tool, "probe", product, product / "target", work / "generated-errors")
    baseline = (HERE.parent / "native-error/darwin-arm64/baseline.txt").read_bytes()
    assert old_errors == new_errors == baseline
    assert len(baseline.splitlines()) == 32
    return {"nativeErrorRows": len(new_errors.splitlines()), "nativeErrorStdoutSha256": sha(new_errors)}


def qualify(work, manifest, original, framework, compiler, reference_target):
    assembly = lane("workspace")
    inputs, product = work / "inputs", work / "product"
    inputs.mkdir()
    descriptor = assembly["stage_workspace"](manifest, inputs)
    staged = sorted(path for path in inputs.rglob("*") if path.is_file())
    assert staged and all(path.suffix in {".json", ".ttl", ".lock"} for path in staged)
    prefix, cli, denied_inputs = sandbox(work, original, compiler, framework, reference_target)
    run(*prefix, "RUSTUP_TOOLCHAIN=1.97.1", cli, "build", "native-workspace",
        "rust", inputs / manifest.name, product, cwd=work)
    run(*prefix, "cargo", "+1.97.1", "build", "--workspace", "--release", "--locked",
        "--offline", cwd=product)
    metadata = json.loads(run("cargo", "+1.97.1", "metadata", "--format-version", "1",
                              "--offline", "--locked", cwd=product))
    packages = {package["name"]: package for package in metadata["packages"]
                if package["id"] in metadata["workspace_members"]}
    assert packages.keys() == descriptor["members"].keys()
    for name, package in packages.items():
        siblings = sorted(dep["name"] for dep in package["dependencies"] if dep["name"] in packages)
        assert siblings == sorted(descriptor["members"][name].get("dependencies", []))
    generated_lock = (product / "Cargo.lock").read_bytes()
    assert sha(generated_lock) == descriptor["cargoLock"]["sha256"]
    manifest_lock = (product / "workspace.lock").read_bytes()
    pinned_workspace_lock = manifest.with_suffix("") / "workspace.lock"
    if pinned_workspace_lock.is_file():
        assert manifest_lock == pinned_workspace_lock.read_bytes()

    generated_copy = copy_tests(work, original, product)

    vault = vault_check(work, "check-vault-values", original, product, work)

    errors = native_error_observations(work, original, product, reference_target)
    reference_path = HERE.parent / "workspace/v7-aarch64-apple-darwin.json"
    reference = json.loads(reference_path.read_bytes())["releaseInfo"]["cases"]
    assert set(reference) == {"marmish", "marmishd"}
    runtime = work / "report-runtime"
    runtime.mkdir()
    env = dict(HOME=str(runtime), TMPDIR=str(runtime), LANG="C", PATH=os.defpath,
               TARGET="override", CARGO_PKG_VERSION="override", MARMISH_BUILD_COMMIT="override",
               MARMISH_BUILD_TARGET="override")
    original_bins = reference_target / "aarch64-apple-darwin/release"
    generated_bins = product / "target/release"
    binaries = {}
    for name, cases in reference.items():
        assert len(cases) == 16
        for case in cases:
            assert "release-info" in case["argv"], case
            for directory in (original_bins, generated_bins):
                call = subprocess.run([directory / name, *case["argv"]], cwd=runtime, env=env,
                                      capture_output=True, input=b"", timeout=30)
                assert (call.returncode, call.stdout, call.stderr) == (
                    case["exit"], case["stdout"].encode(), case["stderr"].encode()), (name, case)
        binaries[name] = len(cases)
    assert not list(runtime.iterdir())

    receipt = {
        "originalCommit": ORIGINAL,
        "frameworkCommit": run("git", "-C", framework, "rev-parse", "HEAD").decode().strip(),
        "platform": "aarch64-apple-darwin",
        "companionRevision": run("git", "-C", ROOT, "rev-parse", "HEAD").decode().strip(),
        "sourceFree": len(denied_inputs) == 5,
        "deniedReadProbes": denied_inputs,
        "verifierSha256": sha(Path(__file__).read_bytes()),
        "compilerSha256": sha(cli.read_bytes()),
        "manifestSha256": sha(manifest.read_bytes()),
        "referenceReleaseInfoSha256": sha(reference_path.read_bytes()),
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "stagedInputs": {str(path.relative_to(inputs)): sha(path.read_bytes()) for path in staged},
        "cargoLockSha256": sha(generated_lock),
        "workspaceLockSha256": sha(manifest_lock),
        "members": sorted(packages),
        "copyCases": generated_copy,
        "vaultRows": vault["vaultRows"],
        "vaultStdoutSha256": vault["vaultStdoutSha256"],
        "vaultPrivateConstructorRejections": 2,
        **errors,
        "binaries": binaries,
    }
    limits = json.loads(run(
        "cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline", "--manifest-path",
        HERE.parent / "canonical-public-data/Cargo.toml", "--bin", "engine-limits-observer",
        "--target-dir", work / "copy-test-tools", "--", original,
        reference_target / "aarch64-apple-darwin/release", product, reference_target,
    ))
    assert len(limits["argvCases"]) == 10
    receipt["processLimitCases"] = limits["libraryCases"]
    receipt["processStartupCases"] = limits["argvCases"]
    receipt["processArtifacts"] = {
        key: limits[key] for key in (
            "originalBinaries", "generatedBinaries", "originalLibraries",
            "generatedLibraries", "observerSha256",
        )
    }
    receipt["processArtifacts"]["faultObserverSha256"] = limits["darwinFaultObserverSha256"]
    return receipt, manifest_lock


def main():
    if not __debug__:
        raise RuntimeError("vault verification requires Python assertions")
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("manifest", "original", "framework", "compiler_target", "reference_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    frameworks = {
        PORTABLE / "workspace-v7-vault-engine.json": FRAMEWORK,
        PORTABLE / "workspace-v7-recovered-vault-engine.json": "6e2d1ef00f286b6132b84e27a61805c659dfd437",
    }
    if args.manifest not in frameworks:
        raise ValueError(f"unsupported vault workspace: {args.manifest}")
    assert run("git", "-C", args.original, "rev-parse", "HEAD").decode().strip() == ORIGINAL
    assert not run("git", "-C", args.original, "status", "--porcelain", "--untracked-files=no")
    assert run("git", "-C", args.framework, "rev-parse", "HEAD").decode().strip() == frameworks[args.manifest]
    assert not run("git", "-C", args.framework, "status", "--porcelain", "--untracked-files=no")
    assert not run("git", "-C", ROOT, "status", "--porcelain", "--untracked-files=all", "--",
                   "oddlang/portable", "verification/whole")
    run("cargo", "+1.97.1", "build", "--bin", "oddlang", "--offline", "--locked",
        "--target-dir", args.compiler_target, cwd=args.framework)
    compiler = args.compiler_target / "debug/oddlang"
    run("cargo", "+1.97.1", "build", "--package", "marmish-native", "--bins", "--release",
        "--offline", "--locked", "--target", "aarch64-apple-darwin",
        "--target-dir", args.reference_target, cwd=args.original)
    system_temp = run("/usr/bin/getconf", "DARWIN_USER_TEMP_DIR").decode().strip()
    with tempfile.TemporaryDirectory(prefix="odd-unified-vault-", dir=system_temp) as temp:
        work = Path(temp).resolve()
        receipt, lock = qualify(work, args.manifest, args.original, args.framework,
                                compiler, args.reference_target)
        if args.manifest.name == "workspace-v7-recovered-vault-engine.json":
            fixed = vault_check(work, "check-vault-fixed-bytes", args.original, work / "product",
                                args.reference_target, "aarch64-apple-darwin", work)
            receipt = json.loads(subprocess.check_output([
                "cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline", "--manifest-path",
                HERE.parent / "canonical-public-data/Cargo.toml", "--target-dir", work / "copy-test-tools",
                "--bin", "recovered-vault-receipt", "--", work / "inputs" / args.manifest.name,
                work / "product/workspace.lock",
            ], input=json.dumps({"receipt": receipt, "fixed": fixed}).encode()))
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    args.receipt.with_name("workspace.lock").write_bytes(lock)


if __name__ == "__main__":
    main()
