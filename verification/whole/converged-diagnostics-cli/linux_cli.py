"""Build the sealed workspace and compare safe help/version cases natively on Linux ARM64."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from compose import HERE, OUTPUT, PORTABLE, compose, load, sha
from verify import FRAMEWORK, IMPORTS, PARSER, ROOT, WORKSPACE, binaries, execute

HOST = "aarch64-unknown-linux-gnu"


def help_or_version(argv):
    return argv[0] == "help" or argv[-1] in ("--help", "-h", "--version", "-V")


def safe_catalog(role):
    legacy = load(HERE.parent / "cli/transcript.json")[role]
    historical = [row["args"] for row in legacy if help_or_version(row["args"])]
    for argv in historical:
        if not (argv in (["--help"], ["--version"]) or len(argv) == 2 and argv[0] == "help"
                and argv[1].isascii() and argv[1].replace("-", "").isalnum()):
            raise ValueError(f"unsupported historical help case: {argv!r}")
    appended = [argv for argv in PARSER["CASES"] if help_or_version(argv)]
    return historical + appended


def confined_cli(launcher, executable, argv, home):
    prefix = launcher + ["--unshare-net", "--unshare-ipc", "--unshare-pid",
                         "--die-with-parent", "--new-session", "--clearenv",
                         "--ro-bind", "/usr", "/usr", "--symlink", "usr/bin", "/bin",
                         "--symlink", "usr/lib", "/lib", "--dev", "/dev",
                         "--ro-bind", str(home), "/home", "--chdir", "/home",
                         "--setenv", "HOME", "/home", "--setenv", "LANG", "C",
                         "--setenv", "PATH", "/usr/bin:/bin"]
    if Path("/lib64").exists():
        prefix += ["--ro-bind", "/lib64", "/lib64"]
    path = "/subject/" + executable.name
    prefix += ["--ro-bind", str(executable), path, "--remount-ro", "/"]
    if launcher[0] == "sudo":
        prefix += ["setpriv", "--reuid", str(os.getuid()), "--regid", str(os.getgid()),
                   "--clear-groups", "--no-new-privs", "--"]
    result = subprocess.run([*prefix, path, *argv], input=b"", capture_output=True, timeout=8)
    if result.returncode not in (0, 255):
        raise ValueError(f"unexpected exit for {argv!r}: {result.returncode} {result.stderr!r}")
    return {"exit": result.returncode, "stdout": result.stdout.decode("ascii"),
            "stderr": result.stderr.decode("ascii")}


def verify(args):
    if (os.uname().sysname, os.uname().machine) != ("Linux", "aarch64"):
        raise ValueError("requires a native ARM64 Linux host")
    rustc = execute("rustc", "+1.97.1", "-vV").decode().splitlines()
    if f"host: {HOST}" not in rustc or f"commit-hash: {IMPORTS['COMPILER']}" not in rustc:
        raise ValueError("requires the selected native Rust toolchain")
    for checkout, revision in ((args.framework, FRAMEWORK), (args.original, IMPORTS["ORIGINAL"])):
        if execute("git", "-C", checkout, "rev-parse", "HEAD").decode().strip() != revision:
            raise ValueError(f"unexpected checkout at {checkout}")
        execute("git", "-C", checkout, "diff", "--quiet", "HEAD")
    compose(PORTABLE, check=True)
    manifest = PORTABLE / f"{OUTPUT}.json"
    lock = (PORTABLE / OUTPUT / "workspace.lock").read_bytes()
    profile = IMPORTS["Candidate"](OUTPUT, args.input_revision, FRAMEWORK, sha(lock))
    host_release = HERE.parent / "unified-buffered" / f"{HOST}.json"
    sealed = load(host_release)
    originals = {role: args.original_bin_dir / role for role in sealed["releaseInfo"]["originalBinaries"]}
    for role, executable in originals.items():
        if sha(executable.read_bytes()) != sealed["releaseInfo"]["originalBinaries"][role]:
            raise ValueError(f"original native Linux binary hash differs: {role}")

    with tempfile.TemporaryDirectory(prefix="odd-linux-cli-") as temporary:
        work = Path(temporary).resolve()
        inputs = work / "inputs"
        inputs.mkdir()
        spec = WORKSPACE["stage_workspace"](manifest, inputs)
        cache = work / "provider-git"
        WORKSPACE["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
        sandbox, env = WORKSPACE["linux_isolation"](work, cache)
        for source in (manifest, args.original / "Cargo.toml", args.framework / "Cargo.toml"):
            if subprocess.run([*sandbox, "/bin/cat", source], env=env, capture_output=True).returncode == 0:
                raise ValueError(f"source isolation unexpectedly read {source}")
        compiler = Path(shutil.copy2(args.compiler, work / "oddlang"))
        product = work / "product"
        target = work / "target"
        execute(*sandbox, compiler, "build", "native-workspace", "rust", inputs / manifest.name,
                product, env=env)
        IMPORTS["verify_product"](product, ROOT, profile)
        sandbox[sandbox.index("--chdir") + 1] = str(product)
        cargo = execute(*sandbox, "cargo", "build", "--release", "--workspace", "--lib", "--bins",
                        "--locked", "--offline", "--target-dir", target, "--message-format=json", env=env)
        generated = binaries(cargo, product, target)
        launcher = sandbox[:sandbox.index("--unshare-net")]
        home = work / "empty-home"
        home.mkdir()
        emitted = {}
        for role, executable in generated.items():
            rows = [{"args": argv, "original": confined_cli(launcher, originals[role], argv, home),
                      "candidate": confined_cli(launcher, executable, argv, home)}
                    for argv in safe_catalog(role)]
            gaps = [row["args"] for row in rows if row["original"] != row["candidate"]]
            if gaps != [["--debug", "rsh", "--help"], ["--user=audit", "rsh", "--help"]]:
                raise ValueError(f"unexpected native Linux CLI differences for {role}: {gaps!r}")
            emitted[role] = {"matches": len(rows) - len(gaps), "cases": len(rows), "rows": rows,
                             "originalSha256": sha(originals[role].read_bytes()),
                             "generatedSha256": sha(executable.read_bytes())}
        if any(home.iterdir()):
            raise ValueError("offline CLI modified the empty home")
        IMPORTS["verify_product"](product, ROOT, profile)
    return {"platform": HOST, "rustc": rustc, "originalRevision": IMPORTS["ORIGINAL"],
            "originalReleaseReceiptSha256": sha(host_release.read_bytes()), "frameworkRevision": FRAMEWORK,
            "compilerSha256": sha(args.compiler.read_bytes()), "workspaceInputRevision": args.input_revision,
            "workspaceLockSha256": profile.lock_sha256, "observations": emitted}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for name in ("compiler", "framework", "original", "original_bin_dir", "receipt"):
        parser.add_argument(name, type=lambda p: Path(p).resolve())
    arguments = parser.parse_args()
    receipt = verify(arguments)
    arguments.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
