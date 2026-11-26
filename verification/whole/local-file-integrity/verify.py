"""Replay read-only descriptor checks using independently created inert files."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
PROFILE = "local-file-integrity-v1"
MAIN = "workspace-v7-canonical-mdk-test-seam.json"
MEMBER = "marmish-native"
MANIFEST = "workspace-v7-local-file-integrity.json"
PACKAGE = "native-local-file-integrity-package.json"
SOURCE = "crates/marmish-native/src/native_manifest.rs"
PIN = "af54339d250290490d62d4475a77539b7e739071"
CASE = "local_file_observations::descriptor_guard_order"
JOURNAL = runpy.run_path(str(HERE.parent / "journal-records/verify.py"))
ASSEMBLY = runpy.run_path(str(HERE.parent / "workspace/verify.py"))


def run(*args, **kwargs):
    return subprocess.check_output(args, **kwargs)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pin(base, path):
    return {"path": path.relative_to(base).as_posix(), "sha256": sha(path.read_bytes())}


def write(path, obj):
    path.write_text(json.dumps(obj, indent=2, sort_keys=True) + "\n")


ORIGINAL_WRAPPER = b"""
#[cfg(test)]
use self::{safe_file as observed_open, read_bounded as observed_read};
#[cfg(test)]
fn observed_verify(path: &Path, bytes: u64, mode: u32, digest: &[u8; 32], check: bool) -> Result<()> {
    verify_file(path, &Asset { name: "plain", sha256: *digest, bytes, mode }, check)
}
"""

GENERATED_WRAPPER = b"""
#[cfg(test)]
use crate::Error;
#[cfg(test)]
use self::{safe_file as observed_open, read_bounded as observed_read, verify_file as observed_verify};
"""


def observe(command, case, cwd, env):
    raw = run(*command, "--", "--exact", case, "--nocapture", cwd=cwd, env=env).decode()
    if (raw.splitlines().count(f"test {case} ... ok") != 1
            or raw.count("test result: ok. 1 passed; 0 failed; 0 ignored;") != 1):
        raise ValueError(f"expected exactly one descriptor test: {raw}")
    pairs = {}
    for prefix, count in [("FILE_CASE=", 10), ("LINK_CASE=", 3)]:
        rows = [line.removeprefix(prefix).split(":", 1) for line in raw.splitlines()
                if line.startswith(prefix)]
        if len(rows) != count or len(dict(rows)) != count:
            raise ValueError(f"missing or repeated observations: {raw}")
        pairs[prefix] = dict(rows)
    return {"guards": pairs["FILE_CASE="], "links": pairs["LINK_CASE="]}


def original_oracle(source, scratch, cache, host):
    checkout = scratch / "original"
    run("git", "clone", "--quiet", "--local", "--no-checkout", source, checkout)
    run("git", "checkout", "--quiet", "--detach", PIN, cwd=checkout)
    entry = checkout / SOURCE
    before = entry.read_bytes()
    old = b"m.nlink() != 1"
    if before.count(old) != 1:
        raise ValueError("could not isolate the descriptor hardlink condition")
    after = before.replace(old, b"!(1..=2).contains(&m.nlink())")
    probe = (HERE / "observe.rs").read_bytes()
    result = []
    for bytes_ in (before, after):
        entry.write_bytes(bytes_ + ORIGINAL_WRAPPER + probe)
        command = ("cargo", "+1.97.1", "test", "--release", "--locked", "--offline",
                   "-p", MEMBER, "--lib", "--target-dir", cache, "--target", host)
        result.append(observe(command, f"native_manifest::{CASE}", checkout, JOURNAL["cargo_env"]()))
    if (result[0]["guards"] != result[1]["guards"]
            or result[0]["links"] != {"oneLink": "accepted", "twoLinks": "descriptor", "threeLinks": "descriptor"}
            or result[1]["links"] != {"oneLink": "accepted", "twoLinks": "accepted", "threeLinks": "descriptor"}):
        raise ValueError("live source mutation did not isolate the link limit")
    return result, [sha(before), sha(after)]


def recover(compiler, observation, work):
    work.mkdir()
    observed = work / "observed.json"
    selection = work / "selection.json"
    write(observed, {"format": "oddlang-read-only-file-observation-v1", **observation["links"]})
    write(selection, {"format": "oddlang-read-only-file-selection-v1",
                      "observationSha256": sha(observed.read_bytes()),
                      "profile": json.loads((PORTABLE / PROFILE / "policy.json").read_bytes())})
    result = work / "recovered"
    run(compiler, "recover", "read-only-file", observed, selection, result)
    return result


def compose(inputs, recovered):
    workspace = json.loads((PORTABLE / MAIN).read_bytes())
    descriptor = workspace["members"][MEMBER]["manifest"]
    native_file = PORTABLE / descriptor["path"]
    native = json.loads(native_file.read_bytes())
    if pin(PORTABLE, native_file) != descriptor or any(
        component["module"] == "file_integrity" for component in native["components"]
    ):
        raise ValueError("unexpected predecessor native package")
    paths = (Path(PROFILE) / "graph.json", Path(PROFILE) / "rust.json")
    (inputs / PROFILE).mkdir(exist_ok=True)
    (inputs / paths[0]).write_bytes((recovered / "graph.json").read_bytes())
    if inputs != PORTABLE:
        (inputs / paths[1]).write_bytes((PORTABLE / paths[1]).read_bytes())
    native["components"].append({
        "kind": "read-only-file", "module": "file_integrity", "reexports": [],
        "crateReexports": ["safe_file", "read_bounded", "verify_file"],
        "graph": pin(inputs, inputs / paths[0]), "overlay": pin(inputs, inputs / paths[1]),
    })
    write(inputs / PACKAGE, native)
    workspace["members"][MEMBER]["manifest"] = pin(inputs, inputs / PACKAGE)
    write(inputs / MANIFEST, workspace)
    return len(native["components"]) - 1


def sealed(args, host, work, scratch, recovered):
    inputs = work / "inputs"
    inputs.mkdir()
    workspace = ASSEMBLY["stage_workspace"](PORTABLE / MAIN, inputs)
    if host == "aarch64-apple-darwin":
        sandbox = runpy.run_path(str(HERE.parent / "unified-vault/verify.py"))["sandbox"]
        prefix, cli, _ = sandbox(work, args.original, args.compiler, args.framework, args.original_target)
        prefix += ["RUSTUP_TOOLCHAIN=1.97.1"]
        env = os.environ
    elif host == "aarch64-unknown-linux-gnu":
        provider_cache = work / "provider-git"
        ASSEMBLY["stage_provider_git_cache"](inputs / workspace["cargoLock"]["path"], provider_cache)
        prefix, env = ASSEMBLY["linux_isolation"](work, provider_cache)
        cli = work / "oddlang"
        shutil.copy2(args.compiler, cli)
    else:
        raise ValueError("expected native ARM64")
    forbidden = [args.original / SOURCE, scratch / "original" / SOURCE, HERE / "observe.rs"]
    for probe in forbidden:
        denied = subprocess.run([*prefix, "/bin/cat", probe], env=env, cwd=work, capture_output=True)
        if (not probe.is_file() or denied.returncode != 1 or denied.stdout or not any(
            reason in denied.stderr for reason in (b"Operation not permitted", b"No such file")
        )):
            raise ValueError(f"failed to deny source at {probe.name}")

    def invocation(cwd):
        cmd = list(prefix)
        if host.endswith("linux-gnu"):
            cmd[cmd.index("--chdir")+1] = str(cwd)
        return cmd

    # Stage data-only alternate graphs. The common Cargo dependencies are identical.
    receipts = []
    for label, graph in zip(("baseline", "mutated"), recovered, strict=True):
        index = compose(inputs, graph)
        if label == "baseline" and (inputs / MANIFEST).read_bytes() != (PORTABLE / MANIFEST).read_bytes():
            raise ValueError("regenerated descriptor differs from the published baseline")
        product = work / label
        run(*invocation(work), cli, "build", "native-workspace", "rust", inputs / MANIFEST, product,
            cwd=work, env=env)
        lock = (product / "workspace.lock").read_bytes()
        seal = json.loads(lock)
        preceding = json.loads((PORTABLE / Path(MAIN).stem / "workspace.lock").read_bytes())
        if (seal["cargoLockSha256"] != preceding["cargoLockSha256"]
                or seal["workspaceCargoManifestSha256"] != preceding["workspaceCargoManifestSha256"]
                or set(seal["members"]) != set(preceding["members"])
                or any(member != seal["members"][name] for name, member in preceding["members"].items()
                       if name != MEMBER)):
            raise ValueError("changed the canonical Cargo lock, workspace root or a separate member")
        if label == "baseline" and lock != (PORTABLE / Path(MANIFEST).stem / "workspace.lock").read_bytes():
            raise ValueError("baseline lock differs from the published seal")
        ASSEMBLY["verify_generated_tree"](product, seal, True, True)
        compile_args = ("--release", "--locked", "--offline", "-p", MEMBER, "--lib",
                        "--target-dir", work / "target")
        run(*invocation(product), "cargo", "build", *compile_args, cwd=product, env=env)
        with (product / MEMBER / f"src/group_{index}/__projection_{index}.rs").open("ab") as module:
            module.write(GENERATED_WRAPPER + (HERE / "observe.rs").read_bytes())
        result = observe((*invocation(product), "cargo", "test", *compile_args),
                         f"file_integrity::__projection_{index}::{CASE}", product, env)
        receipts.append({"workspaceLockSha256": sha(lock), "observations": result,
                         "recovery": json.loads((graph / "recovery.lock").read_bytes())})
    return receipts


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for key in ("framework", "compiler_target", "original", "original_target", "receipt"):
        ap.add_argument(key, type=lambda value: Path(value).resolve())
    ap.add_argument("--framework-revision", required=True)
    ap.add_argument("--prepare", action="store_true")
    args = ap.parse_args()
    for repository, pin_ in [(args.framework, args.framework_revision), (args.original, PIN)]:
        if (run("git", "rev-parse", "HEAD", cwd=repository).decode().strip() != pin_
                or run("git", "status", "--porcelain", cwd=repository)):
            raise ValueError(f"expected clean pin {pin_}")
    if not args.prepare and run("git", "status", "--porcelain", cwd=ROOT):
        raise ValueError("expected clean companion checkout")
    version = run("rustc", "+1.97.1", "-vV").decode()
    host, = [line[6:] for line in version.splitlines() if line.startswith("host: ")]
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework)
    built = args.compiler_target / "debug/oddlang"
    with tempfile.TemporaryDirectory(prefix="odd-file-oracle-", dir="/tmp") as directory:
        scratch = Path(directory).resolve()
        args.compiler = scratch / "oddlang"
        shutil.copy2(built, args.compiler)
        compiler_digest = sha(args.compiler.read_bytes())
        observed, source_digests = original_oracle(args.original, scratch, args.original_target, host)
        graphs = [recover(args.compiler, value, scratch / f"recovery-{i}") for i, value in enumerate(observed)]
        if args.prepare:
            compose(PORTABLE, graphs[0])
            return
        with tempfile.TemporaryDirectory(prefix="odd-file-sealed-") as directory:
            receipts = sealed(args, host, Path(directory).resolve(), scratch, graphs)
    if [value["observations"] for value in receipts] != observed:
        raise ValueError("projected descriptor policy differs from the original observations")
    write(args.receipt, {"format": "marmish-local-file-integrity-v1", "original": PIN,
                         "framework": args.framework_revision, "compilerSha256": compiler_digest,
                         "rustc": version.splitlines(),
                         "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
                         "probeSha256": sha((HERE / "observe.rs").read_bytes()),
                         "originalAndMutantSourceSha256": source_digests,
                         "deniedSourcePaths": 3, "baseline": receipts[0], "mutation": receipts[1]})


if __name__ == "__main__":
    main()
