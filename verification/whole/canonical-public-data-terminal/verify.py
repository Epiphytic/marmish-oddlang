"""Run only frozen root help/version and help subcommand exits under native confinement."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
ROOT = HERE.parents[2]
DARWIN, LINUX = "aarch64-apple-darwin", "aarch64-unknown-linux-gnu"
BASE = "8f945d725dea3f4d4812eb6ea24bc7a1aff8d75b"
PREFIX = "946850f3242203e5901f6fc14ff143adc67b5b55"
ROLES = ("marmish", "marmishd")
OUTPUT = "workspace-v7-canonical-public-data"
MANIFEST = ROOT / "oddlang/portable" / f"{OUTPUT}.json"


def local(directory, name="verify.py"):
    path = WHOLE / directory
    original_path = list(sys.path)
    saved = {key: sys.modules.pop(key) for key in ("compose", "verify") if key in sys.modules}
    sys.path.insert(0, str(path))
    try:
        return runpy.run_path(str(path / name))
    finally:
        sys.path[:] = original_path
        for key in ("compose", "verify"):
            sys.modules.pop(key, None)
        sys.modules.update(saved)


PREVIOUS = local("canonical-linux-imports")
AUDIT = PREVIOUS["audit"]
closure = PREVIOUS["closure"]
NATIVE = local("unified-mdk", "workspace_compat.py")
run, data, digest = NATIVE["run"], NATIVE["data"], NATIVE["sha"]
WORKSPACE = local("workspace")
TOOLKIT = WORKSPACE["envtools"]


def classify(archived):
    _, tool = TOOLKIT.qualification_tool("terminal-case-inventory")
    return json.loads(subprocess.check_output([tool], input=json.dumps(archived).encode()))


def closure_digest(files):
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()


def build(work, scratch, host):
    product, target = work / "candidate", work / "target"
    if host == LINUX:
        prefix, env = WORKSPACE["linux_isolation"](work, work / "providers")
        launcher = prefix[:prefix.index("--unshare-net")]
        prefix[prefix.index("--chdir") + 1] = str(product)
    else:
        template = scratch / "cargo-template"
        template.mkdir()
        readable, env = TOOLKIT.isolation_environment(template, host)
        xcrun = ("xcrun", "--sdk", "macosx", "--find")
        make = Path(TOOLKIT.run(*xcrun, "make").strip())
        env.update(HOME=str(work), CARGO_HOME=str(work / "candidate-cargo"), TMPDIR=str(work / "tmp"),
                   CC=env["CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"], AR=TOOLKIT.run(*xcrun, "ar").strip(),
                   RANLIB=TOOLKIT.run(*xcrun, "ranlib").strip(), MACOSX_DEPLOYMENT_TARGET="11.0",
                   XCRUN_CACHE_DISABLED="1", PATH=f"{make.parent}:{env['PATH']}")
        xcode = next(parent for parent in Path(env["DEVELOPER_DIR"]).parents if parent.suffix == ".app")
        prefix = TOOLKIT.isolated(work, (*readable, xcode), ROOT)
        launcher = None
    denied = subprocess.run([*prefix, "/bin/cat", MANIFEST], cwd=product, env=env, capture_output=True)
    if denied.returncode != 1 or denied.stdout or not any(
        message in denied.stderr for message in (b"Operation not permitted", b"No such file")
    ):
        raise ValueError("build sandbox read the original companion manifest")
    raw = run(*prefix, "cargo", "build", "--release", "--locked", "--offline", "--target", host,
              "--target-dir", target, "--message-format=json", "--workspace", "--lib", "--bins",
              cwd=product, env=env)
    _, tool = TOOLKIT.qualification_tool("terminal-case-inventory")
    selected = json.loads(subprocess.check_output(
        [tool, "linked-binaries", product, target, host], input=raw))
    bins = {role: Path(path) for role, path in selected.items()}
    return bins, launcher, raw


def replay(archived, originals, binaries, scratch, launcher):
    modes = {None: "seatbelt", ("bwrap",): "bwrap", ("sudo", "-n", "bwrap"): "sudo-bwrap"}
    mode = modes[tuple(launcher) if launcher is not None else None]
    _, tool = TOOLKIT.qualification_tool("terminal-case-inventory")
    return json.loads(subprocess.check_output(
        [tool, "replay", mode, originals, binaries, scratch], input=json.dumps(archived).encode()))


def verify(args, scratch):
    host = {("Darwin", "arm64"): DARWIN, ("Linux", "aarch64"): LINUX}.get((platform.system(), platform.machine()))
    if host is None:
        raise ValueError("requires a native ARM64 host")
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    if f"host: {host}" not in version or f"commit-hash: {AUDIT.COMPILER}" not in version:
        raise ValueError("requires exact Rust 1.97.1")
    app = AUDIT.git(ROOT, "rev-parse", "HEAD").decode().strip()
    paths = [
        WHOLE / "canonical-public-data" / f"{host}.json",
        WHOLE / "terminal-prefix" / f"{host}.json",
    ]
    for path, revision in zip(paths, (BASE, PREFIX), strict=True):
        if path.read_bytes() != AUDIT.git(ROOT, "show", f"{revision}:{path.relative_to(ROOT)}"):
            raise ValueError(f"archived native receipt changed: {path.name}")
    base, archived = map(data, paths)
    classify(archived)
    originals = {role: (args.original_bin_dir / role).resolve() for role in ROLES}
    original_hashes = {role: digest(path) for role, path in originals.items()}
    if original_hashes != {role: archived["roles"][role]["originalSha256"] for role in ROLES}:
        raise ValueError("wrong pinned original release executables")

    work = args.sealed_build
    product, target = work / "candidate", work / "target"
    releases = target / host / "release"
    libraries = {
        name: releases / f"libmarmish_{name}.rlib" for name in ("engine", "native")
    }
    library_hashes = {name: digest(path) for name, path in libraries.items()}
    if library_hashes != {name: base["publicData"][f"candidate{name.title()}LibrarySha256"] for name in libraries}:
        raise ValueError("wrong qualified generated libraries")
    raw = (work / "cargo-artifacts.jsonl").read_bytes()
    if hashlib.sha256(raw).hexdigest() != base["artifactLogSha256"]:
        raise ValueError("wrong archived same-workspace Cargo build log")
    files = closure(raw, target, host)
    if len(files) != base["cargoLibraryClosureEntries"] or closure_digest(files) != base["cargoLibraryClosureSha256"]:
        raise ValueError("sealed Cargo library closure changed before linking")
    profile = AUDIT.Candidate(OUTPUT, base["companion"], base["framework"],
                              base["workspace"]["candidateWorkspaceLockSha256"])
    AUDIT.verify_product(product, ROOT, profile)
    binaries, launcher, link_log = build(work, scratch, host)
    if closure(link_log, target, host) != files:
        raise ValueError("binary build linked a different Cargo library closure")
    binary_hashes = {role: digest(path) for role, path in binaries.items()}
    results = replay(archived, args.original_bin_dir, releases, scratch, launcher)
    AUDIT.verify_product(product, ROOT, profile)
    if any(digest(path) != binary_hashes[role] for role, path in binaries.items()) or any(
        digest(path) != original_hashes[role] for role, path in originals.items()
    ) or any(digest(path) != library_hashes[name] for name, path in libraries.items()) or (
        closure(raw, target, host) != files
    ):
        raise ValueError("a native artifact changed during the replay")
    return {
        "format": "marmish-combined-terminal-help-version-v1", "host": host, "rustc": version,
        "companion": app, "framework": base["framework"], "original": archived["original"],
        "workspaceLockSha256": profile.lock_sha256,
        "binaryBuildArtifactLogSha256": hashlib.sha256(link_log).hexdigest(),
        "cargoLibraryClosureSha256": closure_digest(files), "cargoLibraryClosureEntries": len(files),
        "sourceWorkspaceReceiptSha256": digest(paths[0]), "terminalPrefixReceiptSha256": digest(paths[1]),
        "originalBinaries": original_hashes, "candidateBinaries": binary_hashes,
        "candidateLibraries": library_hashes, "roles": results,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("sealed_build", "original_bin_dir", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="marmish-help-version-") as tmp:
        captured = verify(args, Path(tmp).resolve())
    args.receipt.write_text(json.dumps(captured, indent=2) + "\n")
