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

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK = "18552b422e794b5b0db6d215ec73f1e9cd5800e5"


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def lane(name, file="verify.py"):
    return runpy.run_path(str(HERE.parent / name / file))


def sandbox(work, original, compiler, framework):
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    protected = [ROOT.resolve(), original.resolve(), framework.resolve()]
    profile = "(version 1)(allow default)(deny network*)"
    profile += "".join("(deny file-read* (subpath " + json.dumps(str(path)) + "))"
                       for path in protected)
    prefix = ["/usr/bin/sandbox-exec", "-p", profile]
    for file in (ROOT / "Cargo.toml", original / "Cargo.toml", framework / "Cargo.toml"):
        assert file.is_file(), file
        rejected = subprocess.run([*prefix, "/bin/cat", file], capture_output=True)
        assert rejected.returncode and b"Operation not permitted" in rejected.stderr, file
    cli = work / "oddlang"
    shutil.copyfile(compiler, cli)
    cli.chmod(0o755)
    return prefix, cli


def qualify(work, manifest, original, framework, compiler, reference_target, engine):
    assembly = lane("workspace")
    inputs, product = work / "inputs", work / "product"
    inputs.mkdir()
    descriptor = assembly["stage_workspace"](manifest, inputs)
    staged = sorted(path for path in inputs.rglob("*") if path.is_file())
    assert staged and all(path.suffix in {".json", ".ttl", ".lock"} for path in staged)
    prefix, cli = sandbox(work, original, compiler, framework)
    run(*prefix, "/usr/bin/env", "RUSTUP_TOOLCHAIN=1.97.1", cli, "build", "native-workspace",
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

    copy = lane("copy-frame")
    source = run("git", "-C", original, "show",
                 ORIGINAL + ":crates/marmish-stream/src/copy_protocol.rs")
    start = source.index(b"#[cfg(test)]\nmod tests {")
    upstream = source[start:source.index(b"\nmod path_encoding {", start)]
    assert upstream.count(b"#[test]") == 11
    original_copy = copy["tests"](original / "crates/marmish-stream", original / "Cargo.lock",
                                 work / "original-copy", upstream)
    generated_copy = copy["tests"](product / "marmish-stream", product / "Cargo.lock",
                                  work / "generated-copy", upstream)
    assert original_copy == generated_copy

    vault = lane("vault-values")
    original_vault = vault["consumer"](original / "crates/marmish-vault", work / "original-vault",
                                      original / "Cargo.lock", privacy=True)
    generated_vault = vault["consumer"](product / "marmish-vault", work / "generated-vault",
                                       product / "Cargo.lock", privacy=True)
    assert original_vault == generated_vault == vault["expected"](128)

    errors = lane("native-error")
    old_errors = errors["native_probe"](original, reference_target, work / "original-errors")
    new_errors = errors["native_probe"](product, product / "target", work / "generated-errors")
    baseline = (HERE.parent / "native-error/darwin-arm64/baseline.txt").read_bytes()
    assert old_errors == new_errors == baseline
    assert len(baseline.splitlines()) == 32
    reference_path = HERE.parent / "workspace/v7-aarch64-apple-darwin.json"
    reference = json.loads(reference_path.read_bytes())["releaseInfo"]["cases"]
    assert set(reference) == {"marmish", "marmishd"}
    runtime = work / "report-runtime"
    runtime.mkdir()
    env = dict(HOME=str(runtime), TMPDIR=str(runtime), LANG="C", PATH=os.defpath,
               TARGET="override", CARGO_PKG_VERSION="override", MARMISH_BUILD_COMMIT="override",
               MARMISH_BUILD_TARGET="override")
    binaries = {}
    for name, cases in reference.items():
        artifact = product / "target/release" / name
        assert len(cases) == 16
        for case in cases:
            call = subprocess.run([artifact, *case["argv"]], cwd=runtime, env=env,
                                  capture_output=True, input=b"", timeout=30)
            assert (call.returncode, call.stdout, call.stderr) == (
                case["exit"], case["stdout"].encode(), case["stderr"].encode()), (name, case)
        binaries[name] = len(cases)
    assert not list(runtime.iterdir())

    receipt = {
        "originalCommit": ORIGINAL,
        "frameworkCommit": run("git", "-C", framework, "rev-parse", "HEAD").decode().strip(),
        "platform": "aarch64-apple-darwin",
        "sourceFree": True,
        "manifestSha256": sha(manifest.read_bytes()),
        "referenceReleaseInfoSha256": sha(reference_path.read_bytes()),
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "stagedInputs": {str(path.relative_to(inputs)): sha(path.read_bytes()) for path in staged},
        "cargoLockSha256": sha(generated_lock),
        "workspaceLockSha256": sha(manifest_lock),
        "members": sorted(packages),
        "copyCases": generated_copy,
        "vaultRows": len(generated_vault),
        "vaultStdoutSha256": sha(("\n".join(generated_vault) + "\n").encode()),
        "vaultPrivateConstructorRejections": 2,
        "nativeErrorRows": len(new_errors.splitlines()),
        "nativeErrorStdoutSha256": sha(new_errors),
        "binaries": binaries,
    }
    if engine:
        observe = lane("engine-limits", "observe.py")
        old_caller, _ = observe["reference_caller"](work, original, reference_target)
        new_caller, _ = observe["generated_caller"](work, product)
        original_limits = observe["file_limits"](work, old_caller)
        generated_limits = observe["file_limits"](work, new_caller)
        assert original_limits == generated_limits
        receipt["processLimitCases"] = generated_limits
        startup = observe["startup"](work, reference_target / "aarch64-apple-darwin/release",
                                     product / "target/release")
        assert len(startup["argvCases"]) == 10
        receipt["processStartupCases"] = startup["argvCases"]
    return receipt, manifest_lock


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("manifest", "original", "framework", "compiler_target", "reference_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    parser.add_argument("--engine", action="store_true")
    args = parser.parse_args()
    assert run("git", "-C", args.original, "rev-parse", "HEAD").decode().strip() == ORIGINAL
    assert not run("git", "-C", args.original, "status", "--porcelain", "--untracked-files=no")
    assert run("git", "-C", args.framework, "rev-parse", "HEAD").decode().strip() == FRAMEWORK
    assert not run("git", "-C", args.framework, "status", "--porcelain", "--untracked-files=no")
    assert args.manifest.parent == PORTABLE
    run("cargo", "+1.97.1", "build", "--bin", "oddlang", "--offline", "--locked",
        "--target-dir", args.compiler_target, cwd=args.framework)
    compiler = args.compiler_target / "debug/oddlang"
    with tempfile.TemporaryDirectory(prefix="odd-unified-vault-") as temp:
        receipt, lock = qualify(Path(temp).resolve(), args.manifest, args.original, args.framework,
                                compiler, args.reference_target, args.engine)
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    args.receipt.with_name("workspace.lock").write_bytes(lock)


if __name__ == "__main__":
    main()
