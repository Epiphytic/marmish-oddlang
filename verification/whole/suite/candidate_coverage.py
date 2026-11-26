"""Bind selected upstream harnesses to a sealed workspace and count uncovered named tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile

from inventory import PIN
import process_limits

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
CORE = "marmish-core/test/contracts"
STREAM = "marmish-stream/lib/marmish_stream"
NATIVE = "marmish-native/test/process_limits"
FRAMEWORK = "7ed1c29acde2cc4cfdc58c2489935e703470ae70"
MANIFEST = HERE.parents[2] / "oddlang/portable/workspace-v7-vault-engine.json"


def run(*cmd, **kwargs):
    return subprocess.check_output(cmd, **kwargs)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pin(root, revision):
    if run("git", "rev-parse", "HEAD", cwd=root).decode().strip() != revision or run(
        "git", "status", "--porcelain", cwd=root
    ):
        raise ValueError(f"expected clean {revision} at {root}")


def stream_name(compiled):
    prefix, tail = compiled.split("::tests::")
    native = {"copy_caller":"copy_protocol", "wire_caller":"wire"}.get(prefix)
    if native is None:
        match = re.fullmatch(r"(windows|codec)::__projection_\d+", prefix)
        if not match:
            raise ValueError(f"unexpected upstream module mapping: {compiled}")
        native = match[1]
    return f"{native}::tests::{tail}"


def reconcile(inventory, passed):
    harnesses = inventory["harnesses"]
    if set(passed) - set(harnesses):
        raise ValueError("case receipt names an unknown harness")
    rows = {}
    for target, entry in harnesses.items():
        eligible, ignored = set(entry["tests"]), set(entry["ignored"])
        names = passed.get(target, [])
        if len(eligible) != len(entry["tests"]) or len(ignored) != len(entry["ignored"]) or eligible & ignored:
            raise ValueError(f"invalid test inventory: {target}")
        if len(set(names)) != len(names) or set(names) - eligible:
            raise ValueError(f"duplicate or unrecognized test: {target}")
        rows[target] = dict(passed=sorted(names), unverified=sorted(eligible-set(names)),
                            ignored=sorted(ignored), benchmarks=entry["benchmarks"])
    totals = {key:sum(len(row[key]) for row in rows.values())
              for key in ("passed", "unverified", "ignored", "benchmarks")}
    return rows, totals


def qualify(original, framework, compiler_cache, reference_target):
    pin(original, PIN)
    pin(framework, FRAMEWORK)
    version = run("rustc", "+1.97.1", "-vV").decode()
    host, = [line[6:] for line in version.splitlines() if line.startswith("host: ")]
    suite = HERE / f"named-release-{host}.json"
    inventory = json.loads(suite.read_bytes())
    if inventory["upstream"] != PIN or inventory["rustc"] != version.strip() or inventory["selection"] != "native release workspace, default features":
        raise ValueError("upstream inventory profile differs from candidate")
    if sha((original/"Cargo.lock").read_bytes()) != inventory["cargoLockSha256"]:
        raise ValueError("upstream Cargo.lock changed")
    assembly = runpy.run_path(str(WHOLE/"workspace/verify.py"))
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", compiler_cache, cwd=framework)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        inputs, product = work/"inputs", work/"product"
        inputs.mkdir()
        workspace = assembly["stage_workspace"](MANIFEST, inputs)
        compiler = compiler_cache/"debug/oddlang"
        if host == "aarch64-apple-darwin":
            sandbox = runpy.run_path(str(WHOLE/"unified-vault/verify.py"))["sandbox"]
            prefix, cli, _ = sandbox(work, original, compiler, framework, reference_target)
            prefix += ["RUSTUP_TOOLCHAIN=1.97.1"]
            env = os.environ
        elif host == "aarch64-unknown-linux-gnu":
            provider_cache = work/"provider-git"
            assembly["stage_provider_git_cache"](inputs/workspace["cargoLock"]["path"], provider_cache)
            prefix, env = assembly["linux_isolation"](work, provider_cache)
            cli = work/"oddlang"
            shutil.copy2(compiler, cli)
            for forbidden in (original/"Cargo.toml", HERE/"inventory.json", framework/"src/lib.rs"):
                denied = subprocess.run([*prefix, "/bin/cat", forbidden], env=env, capture_output=True)
                if not forbidden.is_file() or not denied.returncode or b"No such file" not in denied.stderr:
                    raise ValueError(f"failed isolation control {forbidden}")
        else:
            raise ValueError(f"unsupported build isolation host {host}")

        def inside(*cmd, cwd):
            args = list(prefix)
            if host.endswith("linux-gnu"):
                args[args.index("--chdir")+1] = str(cwd)
            return run(*args, *cmd, cwd=cwd, env=env)

        inside(cli, "build", "native-workspace", "rust", inputs/MANIFEST.name, product, cwd=work)
        lock = (product/"workspace.lock").read_bytes()
        if lock != (MANIFEST.with_suffix("")/"workspace.lock").read_bytes():
            raise ValueError("generated workspace differs from the published candidate")
        inside("cargo", "build", "--locked", "--offline", "--release", "-p", "marmish-native", "--bins", cwd=product)
        modules = assembly["stage_library_tests"](product, original, workspace, True, True)
        witnessed = {}
        for harness, target in [(CORE, ["-p","marmish-core","--test","contracts"]),
                                (STREAM, ["-p","marmish-stream","--lib"])]:
            raw = inside("cargo","test","--locked","--offline","--release", *target, "--","--format","pretty", cwd=product)
            names = assembly["passing_tests"](raw.decode())
            if harness == STREAM:
                names = [stream_name(n) for n in names]
            if not names or sorted(names) != sorted(inventory["harnesses"][harness]["tests"]):
                raise ValueError(f"complete harness did not pass: {harness}")
            witnessed[harness] = names
        release = product/"target/release"
        libc, = (release/"deps").glob("liblibc-*.rlib")
        rustc = Path(run("rustup","which","--toolchain","1.97.1","rustc").decode().strip())
        processes = process_limits.run(original,rustc,libc,{n:release/n for n in ("marmish","marmishd")},None)
        witnessed[NATIVE] = [processes["test"]]
        rows, totals = reconcile(inventory, witnessed)
        return dict(original=PIN, framework=FRAMEWORK, target=host, profile=inventory["selection"],
                    manifestSha256=sha(MANIFEST.read_bytes()), workspaceLockSha256=sha(lock),
                    inventorySha256=sha(suite.read_bytes()), upstreamModules=modules,
                    nativeProcessTest=processes, harnesses=rows, totals=totals)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","compiler_cache","reference_target","receipt"):
        ap.add_argument(name,type=Path)
    ap.add_argument("--require-complete",action="store_true")
    args = ap.parse_args()
    result = qualify(*(getattr(args,n).resolve() for n in ("original","framework","compiler_cache","reference_target")))
    args.receipt.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
    print(result["target"], result["totals"])
    if args.require_complete and result["totals"]["unverified"]:
        sys.exit(1)
