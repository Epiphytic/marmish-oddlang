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
ROOT_TERMINALS = {("--help",), ("-h",), ("--version",), ("-V",), ("help",)}
HELP_COMMANDS = frozenset("""
copy-receipts put get rsh stream streams stream-reconcile keychain-authorize release-info
install install-status uninstall setup init commands command-status cancel wait output daemon
status open-conversation send received foundation-check command-check help command-evidence
command-cleanup-status command-local-cancel command-prepare-cleanup
""".split())


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


COMBINED = local("canonical-public-data", "workspace_compat.py")
AUDIT = COMBINED["AUDIT"]
run, data = COMBINED["run"], COMBINED["data"]
digest = COMBINED["digest"]
WORKSPACE = local("workspace")
LINUX_CLI = local("converged-diagnostics-cli", "linux_cli.py")
TOOLKIT = WORKSPACE["envtools"]


def classify(archived):
    def admits(argv):
        return tuple(argv) in ROOT_TERMINALS or (
            len(argv) == 2 and argv[0] == "help" and argv[1] in HELP_COMMANDS
        )

    expected = ROOT_TERMINALS | {("help", name) for name in HELP_COMMANDS}
    prepared = {}
    for role in ROLES:
        rec = archived["roles"][role]
        selected = [row for row in rec["cases"] if admits(row["args"])]
        excluded = [row["args"] for row in rec["cases"] if not admits(row["args"])]
        if len(selected) != len(expected) or {tuple(row["args"]) for row in selected} != expected or (
            any(row["exit"] != 0 or row["stderr"] or not row["stdout"] for row in selected)
        ):
            raise ValueError(f"unexpected help/version case set for {role}")
        outside = rec["outsideTerminalProfile"]["args"]
        if admits(outside) or len(excluded) != 46:
            raise ValueError("terminal profile exclusion drift")
        prepared[role] = selected, excluded, outside
    return prepared


closure = COMBINED["PREVIOUS"]["closure"]


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
    denied = subprocess.run([*prefix, "/bin/cat", COMBINED["MANIFEST"]], cwd=product, env=env, capture_output=True)
    if denied.returncode != 1 or denied.stdout or not any(
        message in denied.stderr for message in (b"Operation not permitted", b"No such file")
    ):
        raise ValueError("build sandbox read the original companion manifest")
    raw = run(*prefix, "cargo", "build", "--release", "--locked", "--offline", "--target", host,
              "--target-dir", target, "--message-format=json", "--workspace", "--lib", "--bins",
              cwd=product, env=env)
    rows = [json.loads(line) for line in raw.splitlines()]
    if rows[-1] != {"reason": "build-finished", "success": True}:
        raise ValueError("incomplete binary build")
    bins = {}
    for row in rows:
        if row["reason"] != "compiler-artifact" or row["target"]["kind"] != ["bin"]:
            continue
        role = row["target"]["name"]
        path = Path(row["executable"]).resolve()
        if role not in ROLES or role in bins or row["profile"]["test"] or row["profile"]["opt_level"] != "3" or (
            Path(row["manifest_path"]).resolve() != product / "marmish-native/Cargo.toml"
            or path != target / host / "release" / role
        ):
            raise ValueError(f"unexpected generated release executable: {role}")
        bins[role] = path
    if bins.keys() != set(ROLES):
        raise ValueError("missing original application roles")
    return bins, launcher, raw


def observe(binary, argv, home, launcher):
    if launcher is not None:
        return LINUX_CLI["confined_cli"](launcher, binary, argv, home)
    result = subprocess.run(
        [*TOOLKIT.isolated(home, [binary]), binary, *argv],
        cwd=home, env={"PATH": "/usr/bin:/bin", "LANG": "C", "HOME": str(home)},
        input=b"", capture_output=True, timeout=8, umask=0o077,
    )
    return {"exit": result.returncode, "stdout": result.stdout.decode("ascii"),
            "stderr": result.stderr.decode("ascii")}


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
    selected = classify(archived)
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
    profile = AUDIT.Candidate(COMBINED["OUTPUT"], base["companion"], base["framework"],
                              base["workspace"]["candidateWorkspaceLockSha256"])
    AUDIT.verify_product(product, ROOT, profile)
    binaries, launcher, link_log = build(work, scratch, host)
    if closure(link_log, target, host) != files:
        raise ValueError("binary build linked a different Cargo library closure")
    binary_hashes = {role: digest(path) for role, path in binaries.items()}
    results = {}
    for role, (cases, excluded, outside) in selected.items():
        home = scratch / role
        home.mkdir()
        verified = []
        for row in cases:
            argv = row["args"]
            wanted = {key: value for key, value in row.items() if key != "args"}
            first = observe(originals[role], argv, home, launcher)
            second = observe(binaries[role], argv, home, launcher)
            if first != wanted or second != wanted or any(home.iterdir()):
                raise ValueError(f"unexpected terminal output or filesystem change for {role} {argv!r}")
            verified.append({"args": argv, "exit": second["exit"],
                             "stdoutSha256": hashlib.sha256(second["stdout"].encode("ascii")).hexdigest(),
                             "stderrSha256": hashlib.sha256(second["stderr"].encode("ascii")).hexdigest()})
        results[role] = {"observations": verified, "excludedArchivedRows": excluded,
                         "excludedPostParseControl": outside}
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
