"""Compare source-free terminal CLI roles with each native host's original binaries."""
import argparse
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BASE = "workspace-v7-converged-diagnostics-cli"
OUTPUT = "workspace-v7-terminal-prefix"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"

FRAMEWORK = "4b0e07a97de579adbd779bbd598a532bda9fdb3c"
DARWIN = "aarch64-apple-darwin"
LINUX = "aarch64-unknown-linux-gnu"
ROLES = ("marmish", "marmishd")


def inherited():
    directory = HERE.parent / "converged-diagnostics-cli"
    saved = {name: sys.modules.pop(name) for name in ("compose", "verify") if name in sys.modules}
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / "linux_cli.py"))
    finally:
        sys.path.pop(0)
        for dependency in ("compose", "verify"):
            sys.modules.pop(dependency, None)
        sys.modules.update(saved)


PRIOR = inherited()
load, sha = PRIOR["load"], PRIOR["sha"]
WORKSPACE, IMPORTS = PRIOR["WORKSPACE"], PRIOR["IMPORTS"]
exact_checkout, execute = (PRIOR[key] for key in ("exact_checkout", "execute"))

EXTRA = [
    ["--debug", "rsh", "-h"],
    ["-iaudit", "rsh", "--help"], ["-i", "audit", "rsh", "--help"],
    ["--identity=audit", "rsh", "-h"], ["--identity", "audit", "rsh", "--help"],
    ["-uaudit", "rsh", "--help"], ["-u", "audit", "rsh", "--help"],
    ["--debug", "rsh"], ["--debug", "rsh", "--idle-seconds=0"],
    ["--identity", "rsh", "--help"], ["--help", "rsh"],
    ["--no-such-option", "rsh", "--help"],
    ["--user=audit", "rsh"], ["--user=audit", "rsh", "--version"],
    ["--debug", "--user=audit", "rsh", "-h"], ["--version", "rsh"],
]
# This token parses as a positional in the independent auxiliary grammar.
# Original host validation rejects its leading dash before any operation.
POST_PARSE = ["--debug", "rsh", "--version"]


def corpus(role):
    archived = load(HERE.parent / "cli/transcript.json")[role]
    for row in archived:
        argv = row["args"]
        if not (argv in (["--help"], ["--version"], ["--oddlang-unrecognized-option"])
                or len(argv) == 2 and argv[0] == "help" and argv[1].isascii()
                and argv[1].replace("-", "").isalnum()):
            raise ValueError(f"unexpected archived invocation: {argv!r}")
    return [row["args"] for row in archived] + PRIOR["CLI_CASES"] + EXTRA


def build(args, work, compiler, host, profile):
    manifest = PORTABLE / f"{OUTPUT}.json"
    inputs = work / "inputs"
    inputs.mkdir()
    spec = WORKSPACE["stage_workspace"](manifest, inputs)
    staged = Path(shutil.copy2(compiler, work / "oddlang"))
    product = work / "product"
    if host == LINUX:
        cache = work / "provider-git"
        WORKSPACE["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
        sandbox, env = WORKSPACE["linux_isolation"](work, cache)
        target = work / "target"
    else:
        denied = (ROOT, args.original, args.framework)
        policy = "(version 1) (allow default) (deny network*)" + "".join(
            f" (deny file-read* (subpath {json.dumps(str(path))}))" for path in denied)
        sandbox = ["/usr/bin/sandbox-exec", "-p", policy]
        env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.97.1"}
        target = args.candidate_target or work / "target"
    for source in (manifest, args.original / "Cargo.toml", args.framework / "Cargo.toml"):
        if subprocess.run([*sandbox, "/bin/cat", source], env=env, capture_output=True).returncode == 0:
            raise ValueError(f"source isolation unexpectedly read {source}")
    execute(*sandbox, staged, "build", "native-workspace", "rust", inputs / manifest.name, product, env=env)
    IMPORTS["verify_product"](product, ROOT, profile)
    if host == LINUX:
        sandbox[sandbox.index("--chdir") + 1] = str(product)
        cargo = ["cargo"]
        launcher = sandbox[:sandbox.index("--unshare-net")]
    else:
        cargo = ["cargo", "+1.97.1"]
        launcher = None
    raw = execute(*sandbox, *cargo, "build", "--release", "--workspace", "--lib", "--bins",
                  "--locked", "--offline", "--target-dir", target, "--message-format=json", cwd=product, env=env)
    IMPORTS["verify_product"](product, ROOT, profile)
    return PRIOR["binaries"](raw, product, target), launcher


def observe(executable, argv, home, launcher):
    if launcher is not None:
        return PRIOR["confined_cli"](launcher, executable, argv, home)
    row = PRIOR["observe_cli"](executable, argv, home)
    return {key: value for key, value in row.items() if key != "args"}


def verify(args):
    host = {("Darwin", "arm64"): DARWIN, ("Linux", "aarch64"): LINUX}.get(
        (platform.system(), platform.machine()))
    if host is None:
        raise ValueError("requires native ARM64 Linux or Darwin")
    rustc = execute("rustc", "+1.97.1", "-vV").decode().splitlines()
    if f"host: {host}" not in rustc or f"commit-hash: {IMPORTS['COMPILER']}" not in rustc:
        raise ValueError("requires the exact native Rust toolchain")
    exact_checkout(ROOT, args.input_revision)
    exact_checkout(args.original, ORIGINAL)
    exact_checkout(args.framework, FRAMEWORK)
    for output in (args.compiler_target, args.candidate_target):
        if output is not None and any(output.is_relative_to(root) for root in (ROOT, args.original, args.framework)):
            raise ValueError("build outputs must be external to the input checkouts")
    if host == LINUX and args.candidate_target is not None:
        raise ValueError("Linux builds use a writable target inside the isolated namespace")
    execute("cargo", "+1.97.1", "build", "--release", "--offline", "--locked", "--bin", "oddlang",
            "--target-dir", args.compiler_target, cwd=args.framework)
    compiler = args.compiler_target / "release/oddlang"
    execute("cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline",
            "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
            "--target-dir", args.compiler_target, "--bin", "workspace-product", "--",
            "compose-cli", "terminal-prefix", compiler, args.original, PORTABLE)

    parent = load(PORTABLE / BASE / "workspace.lock")
    lock_path = PORTABLE / OUTPUT / "workspace.lock"
    lock = load(lock_path)
    if (lock["members"].keys() != parent["members"].keys()
            or any(old["dependencies"] != lock["members"][name]["dependencies"]
                   or (name != "marmish-native" and old != lock["members"][name])
                   for name, old in parent["members"].items())
            or any(lock[key] != parent[key] for key in ("cargoLockSha256", "workspaceCargoManifestSha256"))):
        raise ValueError("workspace composition differs outside the native member or changes sibling edges")
    profile = IMPORTS["Candidate"](OUTPUT, args.input_revision, FRAMEWORK, sha(lock_path.read_bytes()))
    witness = HERE.parent / "unified-buffered" / f"{host}.json"
    originals = {role: args.original_bin_dir / role for role in ROLES}
    original_hashes = load(witness)["releaseInfo"]["originalBinaries"]
    if {role: sha(path.read_bytes()) for role, path in originals.items()} != original_hashes:
        raise ValueError("original native binary pin differs")

    with tempfile.TemporaryDirectory(prefix="odd-cli-prefix-") as temp:
        work = Path(temp).resolve()
        generated, launcher = build(args, work, compiler, host, profile)
        rows = {}
        for role in ROLES:
            home = work / role
            home.mkdir()
            witness_rows = []
            for argv in corpus(role):
                expected = observe(originals[role], argv, home, launcher)
                actual = observe(generated[role], argv, home, launcher)
                if actual != expected or expected["exit"] not in (0, 255):
                    raise ValueError(f"{host}/{role} {argv!r} differs: {expected!r} != {actual!r}")
                witness_rows.append({"args": argv, **actual})
            if any(home.iterdir()):
                raise ValueError("CLI changed its empty home")
            original_post_parse = observe(originals[role], POST_PARSE, home, launcher)
            candidate_post_parse = observe(generated[role], POST_PARSE, home, launcher)
            if (original_post_parse["exit"] != 255 or candidate_post_parse["exit"] != 255
                    or "invalid host identity or relay" not in original_post_parse["stderr"]
                    or original_post_parse == candidate_post_parse):
                raise ValueError(f"unexpected post-parse control: {role}")
            rows[role] = {"cases": witness_rows, "originalSha256": original_hashes[role],
                          "generatedSha256": sha(generated[role].read_bytes()),
                          "outsideTerminalProfile": {"args": POST_PARSE, "original": original_post_parse,
                                                     "candidate": candidate_post_parse}}
    return {"platform": host, "original": ORIGINAL, "framework": FRAMEWORK, "workspaceInputRevision": args.input_revision,
            "rustc": rustc, "workspaceLockSha256": profile.lock_sha256, "compilerSha256": sha(compiler.read_bytes()),
            "verifierSha256": sha(Path(__file__).read_bytes()), "originalReleaseReceiptSha256": sha(witness.read_bytes()),
            "roles": rows}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for name in ("framework", "compiler_target", "original", "original_bin_dir", "receipt"):
        parser.add_argument(name, type=lambda p: Path(p).resolve())
    parser.add_argument("--candidate-target", type=lambda p: Path(p).resolve())
    arguments = parser.parse_args()
    receipt = verify(arguments)
    arguments.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
