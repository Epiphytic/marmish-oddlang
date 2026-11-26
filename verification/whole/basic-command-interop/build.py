#!/usr/bin/env python3
"""Build and export the pinned basic-command candidate without the oracle."""

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import platform
import re
import runpy
import shutil
import subprocess
import tempfile
import tomllib


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
WORKSPACE = PORTABLE / "workspace-v7-canonical-offline.json"
COMPANION = "53ece04bad4d12f0a6846ecb652493be7bf92a55"
FRAMEWORK = "8a20ea283d0d43da42f054a7c7774eb9e6a20da7"
RUST = "1.97.1"
HOST = "aarch64-apple-darwin"
ASSEMBLY_FILE = HERE.parent / "workspace/verify.py"
ASSEMBLY = runpy.run_path(str(ASSEMBLY_FILE))
TOOLS = ASSEMBLY["envtools"]


@dataclass(frozen=True)
class BuildInputs:
    workspace: Path
    companion_revision: str
    framework_revision: str


def run(*command, cwd=None, env=None):
    return subprocess.check_output(command, cwd=cwd, env=env, timeout=1200)


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def copy_registry(locks, source, target):
    """Stage archives by lock checksum and unpack them only inside Seatbelt."""
    rows = (row for lock in locks for row in tomllib.loads(lock.read_text())["package"])
    source_cache = source / "cache"
    shutil.copytree(source / "index", target / "index",
                    ignore=shutil.ignore_patterns(".git", ".DS_Store"))
    staged = {}
    for row in rows:
        if "checksum" not in row:
            continue
        if row.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"unexpected registry: {row}")
        filename = f"{row['name']}-{row['version']}.crate"
        if filename in staged:
            if staged[filename] != row["checksum"]:
                raise ValueError(f"inconsistent lock checksums for {filename}")
            continue
        matches = list(source_cache.glob(f"index.crates.io-*/{filename}"))
        if len(matches) != 1 or sha(matches[0]) != row["checksum"]:
            raise ValueError(f"missing or mismatched cached archive: {filename}")
        output = target / "cache" / matches[0].relative_to(source_cache)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(matches[0], output)
        if sha(output) != row["checksum"]:
            raise ValueError(f"copied archive fails its lock checksum: {filename}")
        staged[filename] = row["checksum"]
    return staged


def verified_binaries(messages, product):
    binaries = {}
    for line in messages.splitlines():
        row = json.loads(line)
        if row["reason"] != "compiler-artifact" or row["executable"] is None:
            continue
        if "bin" not in row["target"]["kind"]:
            continue
        manifest = Path(row["manifest_path"]).resolve()
        if not manifest.is_relative_to(product):
            continue
        path = Path(row["executable"]).resolve()
        name = row["target"]["name"]
        if (path != product / "target/release" / name or name in binaries
                or row["fresh"] or row["profile"]["test"]):
            raise ValueError(f"invalid fresh executable artifact: {name}")
        binaries[name] = path
    return binaries


def verify_product(product, workspace):
    pinned = workspace.with_suffix("") / "workspace.lock"
    emitted = product / "workspace.lock"
    if pinned.read_bytes() != emitted.read_bytes():
        raise ValueError("generated workspace lock differs")
    lock = read(emitted)
    if (sha(product / "Cargo.toml") != lock["workspaceCargoManifestSha256"]
            or sha(product / "Cargo.lock") != lock["cargoLockSha256"]):
        raise ValueError("generated workspace manifest or Cargo.lock differs")
    expected = {Path(name) for name in ("Cargo.toml", "Cargo.lock", "workspace.lock")}
    for name, row in lock["members"].items():
        member = product / name
        package_lock = member / "package.lock"
        if (sha(member / "Cargo.toml") != row["workspaceCargoManifestSha256"]
                or sha(package_lock) != row["standalonePackageLockSha256"]):
            raise ValueError(f"generated member manifest or lock differs: {name}")
        files = {path.relative_to(member).as_posix(): sha(path)
                 for path in (member / "src").rglob("*") if path.is_file()}
        if (member / "build.rs").is_file():
            files["build.rs"] = sha(member / "build.rs")
        if files != read(package_lock)["generatedFiles"]:
            raise ValueError(f"generated member sources differ: {name}")
        expected.update(Path(name, path) for path in {*files, "Cargo.toml", "package.lock"})
    actual = set()
    for path in product.rglob("*"):
        relative = path.relative_to(product)
        if relative.parts[0] == "target":
            continue
        if path.is_symlink():
            raise ValueError(f"unexpected generated symlink: {relative}")
        if path.is_file():
            actual.add(relative)
    if actual != expected:
        raise ValueError(f"unexpected generated file inventory: {sorted(actual ^ expected)}")
    return lock


def candidate_roles(binaries, workspace):
    member = read(workspace)["members"]["marmish-native"]["manifest"]
    native = read(PORTABLE / member["path"])
    declared = {item["name"] for item in native["binaries"]}
    if binaries.keys() != declared:
        raise ValueError("first-party executable artifact inventory differs from package")
    roles = {}
    for component in native["components"]:
        if component["kind"] != "cli-surface":
            continue
        binding = read(PORTABLE / component["overlay"]["path"])
        executable = binding["executable"]
        if executable not in declared or executable in roles:
            raise ValueError(f"unexpected CLI binding executable: {executable}")
        roles[executable] = {
            "boundOperations": sorted(operation for operation, iri in binding["operations"].items()
                                      if iri in binding["handlers"] and operation),
            "unboundOperations": sorted(operation for operation, iri in binding["operations"].items()
                                        if iri not in binding["handlers"]),
            "uncoveredExit": binding["uncovered_exit"],
        }
    return roles


def qualify(work, compiler, export, probes, checkout, pins):
    inputs = work / "inputs"
    inputs.mkdir()
    descriptor = ASSEMBLY["stage_workspace"](pins.workspace, inputs)
    staged = {path.relative_to(inputs).as_posix(): sha(path)
              for path in sorted(inputs.rglob("*")) if path.is_file()}
    if any(Path(path).suffix not in {".json", ".ttl", ".lock"} for path in staged):
        raise ValueError("unexpected input file type")
    cargo_lock = inputs / descriptor["cargoLock"]["path"]
    locks = [cargo_lock]
    for member in descriptor["members"].values():
        manifest = inputs / member["manifest"]["path"]
        member_lock = manifest.parent / read(manifest)["cargoLock"]["path"]
        if member_lock not in locks:
            locks.append(member_lock)
    git_sources = lambda lock: {source for _, _, source, _ in ASSEMBLY["external_packages"](lock)
                                if source.startswith("git+")}
    root_git_sources = git_sources(cargo_lock)
    if any(not git_sources(lock) <= root_git_sources for lock in locks):
        raise ValueError("member selects a Git provider absent from workspace lock")
    readable, env = TOOLS.isolation_environment(work, HOST)
    registry_link = Path(env["CARGO_HOME"]) / "registry"
    registry_source = registry_link.resolve(strict=True)
    registry_link.unlink()
    archives = copy_registry(locks, registry_source, registry_link)
    providers = ASSEMBLY["stage_provider_git_cache"](cargo_lock, Path(env["CARGO_HOME"]) / "git")
    developer = Path(env["DEVELOPER_DIR"])
    xcode = next(path for path in developer.parents if path.suffix == ".app")
    for variable, tool in {"CC": "clang", "AR": "ar", "RANLIB": "ranlib", "MAKE": "make"}.items():
        env[variable] = run("xcrun", "--sdk", "macosx", "--find", tool).decode().strip()
    env.update(PATH=f"{Path(env['MAKE']).parent}:{env['PATH']}", MACOSX_DEPLOYMENT_TARGET="11.0",
               XCRUN_CACHE_DISABLED="1", MARMISH_BUILD_COMMIT=ASSEMBLY["ORIGINAL"])
    readable = tuple(path for path in readable if path != registry_source)
    launcher = TOOLS.isolated(work, (*readable, xcode), ROOT, compiler, registry_source, *probes.values())
    with tempfile.TemporaryDirectory(prefix="odd-basic-denial-canary-") as directory:
        canary = Path(directory).resolve() / "outside-build.txt"
        canary.write_text("default-deny probe\n")
        denied_reads = {}
        for name, path in {**probes, "defaultDenyCanary": canary}.items():
            result = subprocess.run([*launcher, "/bin/cat", path], cwd=work, env=env,
                                    capture_output=True, timeout=30)
            if result.returncode != 1 or result.stdout or b"Operation not permitted" not in result.stderr:
                raise ValueError(f"read exclusion failed: {name}")
            denied_reads[name] = {"file": path.name, "sha256": sha(path)}
    tool = work / "oddlang"
    shutil.copy2(compiler, tool)
    product = work / "product"
    run(*launcher, tool, "build", "native-workspace", "rust", inputs / pins.workspace.name, product,
        cwd=work, env=env)
    if (product / "target").exists():
        raise ValueError("found a target directory before compilation")
    lock = verify_product(product, pins.workspace)
    cargo = Path(env["RUSTC"]).with_name("cargo")
    rustc = run(*launcher, env["RUSTC"], "-vV", cwd=work, env=env).decode().splitlines()
    if f"host: {HOST}" not in rustc or f"commit-hash: {TOOLS.RUST_COMMIT}" not in rustc:
        raise ValueError("unexpected native toolchain")
    cargo_version = run(*launcher, cargo, "-V", cwd=work, env=env).decode().strip()
    output = run(*launcher, cargo, "build", "--workspace", "--release", "--locked", "--offline",
                 "--message-format=json", cwd=product, env=env)
    verify_product(product, pins.workspace)
    binaries = verified_binaries(output, product)
    roles = candidate_roles(binaries, pins.workspace)
    receipt = {
        "format": "marmish-basic-command-generated-build-v1",
        "companionInputRevision": pins.companion_revision,
        "buildDriverCheckout": checkout,
        "frameworkRevision": pins.framework_revision,
        "referenceTargetRevision": ASSEMBLY["ORIGINAL"],
        "platform": HOST,
        "osVersion": platform.mac_ver()[0],
        "osBuild": run("sw_vers", "-buildVersion").decode().strip(),
        "rustc": rustc,
        "cargo": cargo_version,
        "xcode": run("xcodebuild", "-version").decode().splitlines(),
        "sdkVersion": run("xcrun", "--sdk", "macosx", "--show-sdk-version").decode().strip(),
        "compilerSha256": sha(tool),
        "builderSha256": sha(Path(__file__)),
        "helperSha256": {
            ASSEMBLY_FILE.relative_to(ROOT).as_posix(): sha(ASSEMBLY_FILE),
            Path(TOOLS.__file__).relative_to(ROOT).as_posix(): sha(Path(TOOLS.__file__)),
        },
        "seatbeltPolicySha256": hashlib.sha256(launcher[2].encode()).hexdigest(),
        "toolDigests": {name: sha(Path(env[name])) for name in ("RUSTC", "CC", "AR", "RANLIB", "MAKE")}
                       | {"cargo": sha(cargo), "rustdoc": sha(Path(env["RUSTDOC"]))},
        "macosDeploymentTarget": env["MACOSX_DEPLOYMENT_TARGET"],
        "generatedBuildCommitField": env["MARMISH_BUILD_COMMIT"],
        "generatedCargoManifestSha256": sha(product / "Cargo.toml"),
        "workspaceLockSha256": sha(product / "workspace.lock"),
        "cargoLockSha256": sha(product / "Cargo.lock"),
        "stagedContracts": staged,
        "registryArchives": archives,
        "gitProviders": providers,
        "deniedReadProbes": denied_reads,
        "readIsolation": "Host Seatbelt default deny for file contents with workspace, system and toolchain allowlist",
        "globalSandboxExceptions": ["file metadata", "mach lookup", "process operations", "sysctl reads"],
        "filesystemTraceCollected": False,
        "members": sorted(lock["members"]),
        "cliSurfaces": roles,
        "artifacts": {name: {"sha256": sha(path), "sizeBytes": path.stat().st_size}
                      for name, path in sorted(binaries.items())},
    }
    with tempfile.TemporaryDirectory(prefix="odd-basic-export-", dir=export.parent) as directory:
        staged_export = Path(directory)
        for name, path in binaries.items():
            destination = staged_export / name
            shutil.copy2(path, destination)
            if sha(destination) != receipt["artifacts"][name]["sha256"]:
                raise ValueError(f"binary digest mismatch after export: {name}")
        for name in ("Cargo.toml", "Cargo.lock", "workspace.lock"):
            shutil.copyfile(product / name, staged_export / name)
        (staged_export / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
        staged_export.rename(export)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument("export", type=lambda value: Path(value).resolve())
    parser.add_argument("--deny", nargs=2, action="append", metavar=("LABEL", "FILE"), required=True)
    parser.add_argument("--workspace", help="Portable workspace descriptor basename")
    parser.add_argument("--companion-revision", help="Companion commit containing the portable closure")
    parser.add_argument("--framework-revision", help="Framework compiler commit")
    args = parser.parse_args()
    supplied = (args.workspace, args.companion_revision, args.framework_revision)
    pins = BuildInputs(WORKSPACE, COMPANION, FRAMEWORK)
    if any(supplied):
        if not all(supplied) or not re.fullmatch(r"workspace-[a-z0-9-]+\.json", args.workspace):
            parser.error("a candidate requires a workspace basename and both commit pins")
        if not all(re.fullmatch(r"[0-9a-f]{40}", revision) for revision in supplied[1:]):
            parser.error("candidate revisions must use full lowercase Git IDs")
        pins = BuildInputs(PORTABLE / args.workspace, args.companion_revision, args.framework_revision)
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        parser.error("requires native macOS ARM64")
    if args.export.exists():
        parser.error("export must be a new directory")
    if run("git", "-C", args.framework, "rev-parse", "HEAD").decode().strip() != pins.framework_revision:
        parser.error("requires the pinned Oddlang compiler checkout")
    if run("git", "-C", args.framework, "status", "--porcelain", "--untracked-files=all"):
        parser.error("compiler checkout must be clean")
    if (run("git", "-C", ROOT, "diff", "--name-only", pins.companion_revision, "--", "oddlang/portable")
            or run("git", "-C", ROOT, "status", "--porcelain", "--untracked-files=all", "--",
                   "oddlang/portable", "verification/whole")):
        parser.error("build driver checkout must be clean and the portable inputs must match the companion pin")
    checkout = run("git", "-C", ROOT, "rev-parse", "HEAD").decode().strip()
    probes = {label: Path(value).resolve(strict=True) for label, value in args.deny}
    if len(probes) != len(args.deny) or not all(path.is_file() for path in probes.values()):
        parser.error("denial probes must be uniquely labeled existing files")
    required = {"originalSource", "originalBinary", "oracleFixture"}
    if not required <= probes.keys():
        parser.error(f"missing denial probes: {', '.join(sorted(required - probes.keys()))}")
    built_in_probes = {
        "companionContract": pins.workspace,
        "compilerSource": args.framework / "src/lib.rs",
        "previousQualificationOracle": HERE.parent / "canonical-workspace/aarch64-apple-darwin.json",
    }
    if probes.keys() & (built_in_probes.keys() | {"defaultDenyCanary"}):
        parser.error("denial probe label is reserved")
    probes.update(built_in_probes)
    with tempfile.TemporaryDirectory(prefix="odd-basic-compiler-") as target:
        run("cargo", f"+{RUST}", "build", "--release", "--locked", "--offline", "--bin", "oddlang",
            "--target-dir", target, cwd=args.framework)
        with tempfile.TemporaryDirectory(prefix="odd-basic-build-") as directory:
            qualify(Path(directory).resolve(), Path(target).resolve() / "release/oddlang",
                    args.export, probes, checkout, pins)


if __name__ == "__main__":
    main()
