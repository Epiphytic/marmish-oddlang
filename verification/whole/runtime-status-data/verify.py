"""Qualify RuntimeStatus through data-only callers of sealed native packages."""
import argparse
import fcntl
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

BASE = "workspace-v7-canonical-offline"
MEMBER = "marmish-native"
OUTPUT = "workspace-v7-canonical-runtime-status"

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
SOURCE = Path("crates/marmish-native/src/mdk_adapter.rs")
PROBE = HERE / "probe.rs"


def lane(name, script="verify.py"):
    previous = sys.modules.pop("compose", None)
    directory = HERE.parent / name
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / script))
    finally:
        sys.path.pop(0)
        sys.modules.pop("compose", None)
        if previous is not None:
            sys.modules["compose"] = previous


SIX = lane("unified-mdk", "workspace_compat.py")
JOURNAL = SIX["JOURNAL"]
run, sha, data = (SIX[name] for name in ("run", "sha", "data"))
ORIGINAL = SIX["ORIGINAL"]


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (PORTABLE / OUTPUT / "workspace.lock").read_bytes():
        raise ValueError("emitted lock differs from pinned RuntimeStatus workspace")
    new, old = data(lock), data(PORTABLE / BASE / "workspace.lock")
    if any(new[key] != old[key] for key in ("format", "cargoLockSha256", "workspaceCargoManifestSha256")):
        raise ValueError("workspace Cargo configuration changed")
    if new["members"].keys() != old["members"].keys():
        raise ValueError("workspace member set changed")
    for name, row in new["members"].items():
        prior = old["members"][name]
        preserve = ("dependencies", "workspaceCargoManifestSha256") if name == MEMBER else row.keys()
        if any(row[key] != prior[key] for key in preserve):
            raise ValueError(f"unexpected inherited member change: {name}")
    SIX["check_member_files"](product, new)
    return new


def cargo_reference(source, target, *options):
    return run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline",
               "--lib", "-p", MEMBER, "--message-format=json", "--target-dir", target,
               *options, cwd=source, env=JOURNAL["cargo_env"]())


def caller(work, build, native=None):
    _, tool = SIX["workspace_tool"]()
    projected = ["--native", native] if native is not None else []
    observation, digest, serde = json.loads(run(tool, "runtime-status-caller", work, *projected, input=build))
    return observation, digest, Path(serde)


def baseline_expected():
    cases = [("", 0, 0, 0), ("snow\n雪\0", 1, 255, 256), ("limit", 2**64 - 1, 2**64 - 2, 2**63 - 1)]
    return [json.dumps(dict(zip(("identity", "conversations", "received", "pending"), case)),
                       ensure_ascii=False, separators=(",", ":")) for case in cases] + ["usize:64"]


def verify(args, host, work, scratch):
    assembly = lane("workspace")
    run("cargo", "+1.97.1", "build", "--offline", "--locked", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
    compiler = work / "oddlang"
    shutil.copy2(args.compiler_target / "debug/oddlang", compiler)
    inputs = work / "inputs"
    inputs.mkdir()
    manifest = PORTABLE / (OUTPUT + ".json")
    spec = assembly["stage_workspace"](manifest, inputs)
    provider_cache = work / "providers"
    providers = assembly["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], provider_cache)
    prefix, env = JOURNAL["sandbox"](assembly, work, (
        ROOT, args.original, args.framework, args.original_target, args.compiler_target, scratch,
    ), provider_cache)

    def invoke(*argv, cwd=work):
        command = list(prefix)
        if sys.platform == "linux":
            command[command.index("--chdir") + 1] = str(cwd)
        return run(*command, *argv, cwd=cwd, env=env)

    denied = [args.original / SOURCE, args.framework / "src/lib.rs", PROBE]
    for path in denied:
        proc = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if proc.returncode != 1 or proc.stdout or not any(
            item in proc.stderr for item in (b"Operation not permitted", b"No such file or directory")
        ):
            raise ValueError(f"source denial failed: {path}")

    recovery = scratch / "status-recovery"
    SIX["product_witness"]("runtime-status-inputs", compiler, args.original, recovery)
    recovered = recovery / "recovered"
    mutant = recovery / "mutated-original"
    mutated = recovery / "recovered-mutation"
    mutation_inputs = work / "mutation-inputs"
    mutation_inputs.mkdir()
    for name in ("graph.json", "rust.json"):
        shutil.copyfile(mutated / name, mutation_inputs / name)
    staged = {p.relative_to(work).as_posix(): sha(p)
              for folder in (inputs, mutation_inputs) for p in sorted(folder.rglob("*")) if p.is_file()}
    if any(Path(name).suffix not in {".json", ".ttl", ".lock"} for name in staged):
        raise ValueError("source input in package gate")

    product = work / "product"
    invoke(compiler, "build", "native-workspace", "rust", inputs / manifest.name, product)
    locked = check_product(product)
    build = invoke("cargo", "build", "--lib", "-p", MEMBER, "--release", "--locked", "--offline",
                   "--message-format=json", cwd=product)
    generated, generated_library, serde = caller(scratch / "generated", build)
    projection = work / "mutation-projection"
    invoke(compiler, "build", "public-records", "rust", mutation_inputs / "graph.json",
           mutation_inputs / "rust.json", projection)
    changed_native = projection / "libmarmish_native.rlib"
    run("rustc", "+1.97.1", "--edition=2024", "--crate-type=rlib", "--crate-name=marmish_native",
        "-L", f"dependency={serde.parent}", "--extern", f"serde={serde}", projection / "records.rs", "-o", changed_native)
    changed, changed_library, _ = caller(scratch / "generated-mutation", build, changed_native)
    args.original_target.mkdir(parents=True, exist_ok=True)
    # Cargo can report the original artifact as fresh for a relocated source tree.
    # Partition the two models before building and hold the lock through caller linking.
    cache = args.original_target / "runtime-status" / ORIGINAL
    baseline_target = cache / sha(args.original / SOURCE)
    mutation_target = cache / sha(mutant / SOURCE)
    with (args.original_target / ".oddlang-runtime-status-reference.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        originals = {}
        for case, flags in (("default", ()), ("installed-checks", ("--no-default-features", "--features", "installed-checks"))):
            actual, lib, _ = caller(scratch / case, cargo_reference(args.original, baseline_target, *flags))
            if actual != generated or actual["jsonLines"] != baseline_expected():
                raise ValueError(f"RuntimeStatus caller mismatch for {case}: {actual!r} != {generated!r}")
            originals[case] = lib
        reference_mutation, mutation_library, _ = caller(
            scratch / "original-mutation", cargo_reference(mutant, mutation_target),
        )
    observed = [json.loads(line) for line in changed["jsonLines"][:-1]]
    if changed != reference_mutation or changed == generated or changed["diagnostics"] != generated["diagnostics"]:
        raise ValueError("changed source and changed projection failed to track record ordering")
    if any(list(row) != ["identity", "received", "conversations", "pending"] for row in observed):
        raise ValueError("mutation did not change exact serialized field order")
    if [json.loads(line) for line in baseline_expected()[:-1]] != observed:
        raise ValueError("mutation altered values alongside ordering")

    return {
        "format": "marmish-runtime-status-external-data-v1", "host": host, "original": ORIGINAL,
        "framework": args.framework_revision, "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "compilerSha256": sha(compiler), "sourceSha256": sha(args.original / SOURCE), "probeSha256": sha(PROBE),
        "manifestSha256": sha(manifest), "workspaceLockSha256": sha(product / "workspace.lock"),
        "cargoLockSha256": locked["cargoLockSha256"],
        "unchangedMemberLocks": sorted(set(locked["members"]) - {MEMBER}),
        "sourceDenials": ["original-mdk-adapter", "framework-source", "independent-native-caller"],
        "externalGitProviders": providers, "stagedInputs": staged,
        "originalNativeLibraries": originals, "generatedNativeLibrarySha256": generated_library,
        "originalMutantLibrarySha256": mutation_library, "projectedMutantLibrarySha256": changed_library,
        "recovery": data(recovered / "recovery.lock"), "mutationRecovery": data(mutated / "recovery.lock"),
        "observations": generated, "observationsAfterNativeFieldReorder": changed,
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        p.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target"):
        p.add_argument("--" + name, required=True, type=lambda path: Path(path).resolve())
    p.add_argument("--framework-revision", required=True)
    args = p.parse_args()
    for checkout, rev in ((args.framework, args.framework_revision), (args.original, ORIGINAL), (ROOT, None)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=checkout) or (
            rev and run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() != rev
        ):
            raise ValueError(f"expected clean pinned checkout: {checkout}")
    SIX["check_api_release"]("canonical-runtime-status", PORTABLE)
    host, = [line.removeprefix("host: ") for line in run("rustc", "+1.97.1", "-vV").decode().splitlines()
             if line.startswith("host: ")]
    if host not in SIX["PLATFORMS"]:
        raise ValueError(f"unqualified native host: {host}")
    with tempfile.TemporaryDirectory(prefix="odd-runtime-status-sealed-") as tmp, tempfile.TemporaryDirectory(
        prefix="odd-runtime-status-observer-",
    ) as outer:
        result = verify(args, host, Path(tmp).resolve(), Path(outer).resolve())
    args.receipt.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
