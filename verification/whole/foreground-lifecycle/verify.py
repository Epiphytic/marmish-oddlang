"""Seal and exercise the native foreground provider package."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / "native-foreground-lifecycle-package.json"
LOCK = PORTABLE / "native-foreground-lifecycle-v1/package.lock"
FRAMEWORK = "2b877caa69876142311a570d5eff4b893b37ace5"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
WORKSPACE_GATE = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
envtools = WORKSPACE_GATE["envtools"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_pin(repo, revision):
    actual = envtools.run("git", "-C", repo, "rev-parse", "HEAD").strip()
    if actual != revision or envtools.run("git", "-C", repo, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError(f"expected clean checkout at {revision}: {repo}")


def stage(root):
    package = json.loads(MANIFEST.read_bytes())
    root.mkdir()
    pins = [package["cargoLock"]]
    pins.extend(part[key] for part in package["components"] for key in ("graph", "overlay"))
    for pin in pins:
        source = (PORTABLE / pin["path"]).resolve()
        if not source.is_relative_to(PORTABLE) or sha(source) != pin["sha256"]:
            raise ValueError("invalid native package input")
        copy = root / source.relative_to(PORTABLE)
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, copy)
    shutil.copyfile(MANIFEST, root / MANIFEST.name)
    return root / MANIFEST.name, root / package["cargoLock"]["path"]


def wrong_storage_identity(manifest):
    package = json.loads(manifest.read_bytes())
    foreground = next(part for part in package["components"] if part["kind"] == "foreground-profile-owner")
    graph = json.loads((manifest.parent / foreground["graph"]["path"]).read_bytes())
    graph["lifecycle"]["storage_profile"] += ":incompatible"
    altered = manifest.with_name("wrong-store-graph.json")
    altered.write_text(json.dumps(graph))
    foreground["graph"] = {"path": altered.name, "sha256": sha(altered)}
    output = manifest.with_name("wrong-store-package.json")
    output.write_text(json.dumps(package))
    return output


def isolation(work, framework, original, binaries, provider_cache):
    if sys.platform == "linux":
        return WORKSPACE_GATE["linux_isolation"](work, provider_cache)
    readable, env = envtools.isolation_environment(work, "aarch64-apple-darwin")
    find = ["xcrun", "--sdk", "macosx", "--find"]
    make = Path(envtools.run(*find, "make").strip())
    env.update({
        "CC": env["CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"],
        "AR": envtools.run(*find, "ar").strip(),
        "RANLIB": envtools.run(*find, "ranlib").strip(),
        "PATH": f"{make.parent}:{env['PATH']}",
        "MACOSX_DEPLOYMENT_TARGET": "11.0",
        "XCRUN_CACHE_DISABLED": "1",
    })
    developer = Path(env["DEVELOPER_DIR"])
    xcode = next(parent for parent in developer.parents if parent.suffix == ".app")
    (Path(env["CARGO_HOME"]) / "git").symlink_to(provider_cache, target_is_directory=True)
    return envtools.isolated(work, (*readable, xcode), framework, original, ROOT, binaries), env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "binaries", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    check_pin(args.framework, FRAMEWORK)
    check_pin(args.original, ORIGINAL)
    revision = envtools.run("git", "-C", ROOT, "rev-parse", "HEAD").strip()
    check_pin(ROOT, revision)
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary).resolve()
        manifest, cargo_lock = stage(work / "graph-inputs")
        external = WORKSPACE_GATE["external_packages"]
        original_lock = args.original / "Cargo.lock"
        if sha(original_lock) != "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3":
            raise ValueError("the original lock differs")
        if not external(cargo_lock) or not external(cargo_lock) <= external(original_lock):
            raise ValueError("provider package expands beyond the original dependency closure")
        cache = work / "provider-cache"
        providers = WORKSPACE_GATE["stage_provider_git_cache"](cargo_lock, cache)
        envtools.run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
                     "--target-dir", work / "compiler", cwd=args.framework)
        compiler = work / "compiler/debug/oddlang"
        sandbox, env = isolation(work, args.framework, args.original, args.binaries, cache)

        def isolated(*command, cwd=work):
            prefix = list(sandbox)
            if sys.platform == "linux":
                prefix[prefix.index("--chdir") + 1] = str(cwd)
            return envtools.run(*prefix, *command, cwd=cwd, env=env)

        for path in (args.original / "Cargo.toml", args.framework / "src/main.rs",
                     args.binaries / "marmish", args.binaries / "marmishd", HERE / "observe.py"):
            process = subprocess.run([*sandbox, "/bin/cat", path], env=env, cwd=work, capture_output=True)
            if process.returncode == 0 or not any(failure in process.stderr for failure in (
                    b"Operation not permitted", b"No such file or directory")):
                raise ValueError(f"build can read excluded file: {path}")
        incorrect = wrong_storage_identity(manifest)
        rejected = work / "wrong-store"
        envtools.run(*sandbox, compiler, "build", "native-package", "rust", incorrect, rejected,
                     cwd=work, env=env, fail="foreground retained store binding differs")
        if rejected.exists():
            raise ValueError("invalid storage link published a package")
        package = work / "native"
        isolated(compiler, "build", "native-package", "rust", manifest, package)
        sealed = package / "package.lock"
        if sealed.read_bytes() != LOCK.read_bytes() or sha(package / "Cargo.lock") != sha(cargo_lock):
            raise ValueError("generated package or Cargo lock differs from the release")
        observed = {path.relative_to(package).as_posix(): sha(path)
                    for path in (package / "src").rglob("*") if path.is_file()}
        if observed != json.loads(sealed.read_bytes())["generatedFiles"]:
            raise ValueError("generated source inventory differs")
        isolated("cargo", "build", "--locked", "--release", "--offline", cwd=package)
        release = package / "target/release"
        dependencies = release / "deps"
        inspector = work / "inspector"
        command = ["rustc", "+1.97.1", "--edition", "2024", "-L", f"dependency={dependencies}"]
        for crate in ("rusqlite", "serde_json", "zeroize"):
            native, = dependencies.glob(f"lib{crate}-*.rlib")
            command.extend(("--extern", f"{crate}={native}"))
        envtools.run(*command, HERE.parent / "status/encrypted_store_inspect.rs", "-o", inspector)
        evidence = work / "observations.json"
        envtools.run(sys.executable, "-B", HERE / "observe.py", args.binaries, release, inspector, evidence,
                     "--framework-revision", FRAMEWORK)
        result = json.loads(evidence.read_bytes())
        result.update({"companionRevision": revision, "cargoLockSha256": sha(cargo_lock),
                       "packageLockSha256": sha(sealed), "gitProviders": providers,
                       "observerSha256": sha(HERE / "observe.py"), "badStorageBindingRejected": True})
    with args.receipt.open("x") as stream:
        stream.write(json.dumps(result, sort_keys=True, indent=2) + "\n")


if __name__ == "__main__":
    main()
