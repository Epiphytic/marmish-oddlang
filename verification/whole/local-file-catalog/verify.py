"""Run one unchanged pinned directory test and an inert-file differential corpus."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

from compose import FILE, HERE, MEMBER, ORIGINAL, OUTPUT, PACKAGE, PORTABLE, SCOPE, compose

SOURCE = FILE["SOURCE"]
ASSEMBLY, JOURNAL, sha, run = (FILE[name] for name in ("ASSEMBLY", "JOURNAL", "sha", "run"))
CASE = "versioned_directory_discovery_supports_both_families_and_rejects_ambiguity"
OBSERVATION_CASE = "inert_catalog_guard_observations"
SAMPLE_SHA = "52f7f2f6dc0c0da5451eb11d5219824654b0b19b9204d91d9df40d3f564a25fd"
TEST_SHA = "7daed3cc184642817d58cd5e56d620ce578a8124504d7c112c2f11324c5189f0"
WRAPPER = b"""
#[cfg(test)]
mod tests {
    use super::{AssetNaming, NAMES, verify_directory};
    use crate::native_manifest::{Manifest, TARGETS};
    use sha2::{Digest, Sha256};
    use std::fs;
"""


def original_parts(data):
    sample = b"    fn sample() -> String {"
    needle = f"    #[test]\n    fn {CASE}()".encode()
    if data.count(sample) != 1 or data.count(needle) != 1 or not data.endswith(b"\n}\n"):
        raise ValueError("missing pinned manifest test seam")
    begin = data.index(sample)
    helper = data[begin:data.index(b"    #[test]\n", begin)]
    body = data[data.index(needle):-3]
    if (sha(helper), sha(body)) != (SAMPLE_SHA, TEST_SHA):
        raise ValueError("unchanged original case digest mismatch")
    return helper, body


def cargo_case(prefix, extra, case, cwd, env):
    cmd = (*prefix, "cargo", "test", "--locked", "--offline", "--release", "-p", MEMBER,
           "--lib", *extra, "--", "--exact", case, "--nocapture")
    observed = run(*cmd, env=env, cwd=cwd).decode()
    if (observed.splitlines().count(f"test {case} ... ok") != 1
            or observed.count("test result: ok. 1 passed; 0 failed; 0 ignored;") != 1):
        raise ValueError(f"named test did not execute once: {observed}")
    cases = [line.removeprefix("CATALOG_CASE=").split(":", 1) for line in observed.splitlines()
             if line.startswith("CATALOG_CASE=")]
    if len(cases) != len(dict(cases)):
        raise ValueError("duplicated independent catalog observations")
    return dict(cases)


def reference(original, original_target, host, scratch, probe):
    clone = scratch / "original"
    run("git", "clone", "--quiet", "--local", "--no-checkout", original, clone)
    run("git", "checkout", "--quiet", "--detach", ORIGINAL, cwd=clone)
    path = clone / SOURCE
    bytes_ = path.read_bytes()
    helper, body = original_parts(bytes_)
    path.write_bytes(bytes_[:-2] + b"\n" + probe + b"\n}\n")
    environment = JOURNAL["cargo_env"]()
    extra = ("--target-dir", original_target, "--target", host)
    # The local original test uses the pinned native toolchain. The sandbox has the same one on PATH.
    name = "native_manifest::tests::"
    command_env = dict(environment, RUSTUP_TOOLCHAIN="1.97.1")
    cargo_case((), extra, name + CASE, clone, command_env)
    cases = cargo_case((), extra, name + OBSERVATION_CASE, clone, command_env)
    if len(cases) != 12:
        raise ValueError("original catalog corpus is incomplete")
    return helper, body, cases


def generated(args, host, work, scratch, helper, body, probe):
    inputs = work / "inputs"
    inputs.mkdir()
    workspace = ASSEMBLY["stage_workspace"](PORTABLE / OUTPUT, inputs)
    if host == "aarch64-apple-darwin":
        sandbox = runpy.run_path(str(HERE.parent / "unified-vault/verify.py"))["sandbox"]
        prefix, compiler, _ = sandbox(work, args.original, args.compiler, args.framework, args.original_target)
        prefix += ["RUSTUP_TOOLCHAIN=1.97.1"]
        env = os.environ
    elif host == "aarch64-unknown-linux-gnu":
        provider = work / "provider-git"
        ASSEMBLY["stage_provider_git_cache"](inputs / workspace["cargoLock"]["path"], provider)
        prefix, env = ASSEMBLY["linux_isolation"](work, provider)
        compiler = work / "oddlang"
        shutil.copy2(args.compiler, compiler)
    else:
        raise ValueError("expected an ARM64 native platform")

    def prefix_in(cwd):
        command = list(prefix)
        if host.endswith("linux-gnu"):
            command[command.index("--chdir")+1] = str(cwd)
        return command

    denied_paths = (args.original / SOURCE, scratch / "original" / SOURCE, HERE / "observe.rs")
    for path in denied_paths:
        denied = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if not path.is_file() or denied.returncode != 1 or denied.stdout or not any(
            reason in denied.stderr for reason in (b"No such file", b"Operation not permitted")
        ):
            raise ValueError(f"source isolation failure: {path}")

    def build(product):
        run(*prefix_in(work), compiler, "build", "native-workspace", "rust", inputs / OUTPUT,
            product, cwd=work, env=env)
        lock = (product / "workspace.lock").read_bytes()
        ASSEMBLY["verify_generated_tree"](product, json.loads(lock), True, True)
        return lock

    product = work / "product"
    lock = build(product)
    if lock != (PORTABLE / Path(OUTPUT).stem / "workspace.lock").read_bytes():
        raise ValueError("workspace lock differs from the pinned catalog candidate")
    spec = json.loads((inputs / PACKAGE).read_bytes())
    components = spec["components"]
    target, = (i for i, c in enumerate(components) if c["kind"] == "local-file-catalog")
    group = next(i for i, c in enumerate(components) if c["module"] == components[target]["module"])
    location = Path(MEMBER) / f"src/group_{group}/__projection_{target}.rs"
    qualified = f"native_manifest::__projection_{target}::tests::"
    extra = ("--target-dir", work / "cache")
    run(*prefix_in(product), "cargo", "build", "--locked", "--offline", "--release",
        "-p", MEMBER, "--lib", *extra, cwd=product, env=env)
    with (product / location).open("ab") as leaf:
        leaf.write(WRAPPER + helper + body + b"\n" + probe + b"\n}\n")
    cargo_case(prefix_in(product), extra, qualified + CASE, product, env)
    cases = cargo_case(prefix_in(product), extra, qualified + OBSERVATION_CASE, product, env)

    # Remove one catalog family in the pinned semantic input and rerun the same case.
    graph_path = inputs / SCOPE / "graph.json"
    graph = json.loads(graph_path.read_bytes())
    removed = graph["prefixes"].pop()
    FILE["write"](graph_path, graph)
    components[target]["graph"] = FILE["pin"](inputs, graph_path)
    FILE["write"](inputs / PACKAGE, spec)
    workspace["members"][MEMBER]["manifest"] = FILE["pin"](inputs, inputs / PACKAGE)
    FILE["write"](inputs / OUTPUT, workspace)
    changed = work / "mutation"
    mutant_lock = build(changed)
    if lock == mutant_lock:
        raise ValueError("deleting a name family did not affect the lock")
    with (changed / location).open("ab") as leaf:
        leaf.write(WRAPPER + helper + body + b"\n}\n")
    source = (changed / location).read_text()
    case_start = source.index(f"fn {CASE}()")
    decoy_setup = source.index('fs::write(root.path().join("marmish-0.3.invalid.manifest")', case_start)
    assertion = source.index("assert!(verify_directory(", decoy_setup)
    next_removal = source.index("fs::remove_file(&path)", decoy_setup)
    if not decoy_setup < assertion < next_removal:
        raise ValueError("cannot locate the upstream ambiguity assertion")
    line = source.count("\n", 0, assertion) + 1
    column = assertion - source.rindex("\n", 0, assertion)
    panic = f"panicked at {location}:{line}:{column}:\n"
    message = "assertion failed: verify_directory(root.path(), AssetNaming::VersionedDownloads, pin).is_err()"
    failing = subprocess.run(
        [*prefix_in(changed), "cargo", "test", "--locked", "--offline", "--release", "-p", MEMBER,
         "--lib", *extra, "--", "--exact", qualified + CASE],
        cwd=changed, env=env, capture_output=True, text=True)
    if (failing.returncode != 101 or f"test {qualified}{CASE} ... FAILED" not in failing.stdout
            or panic + message not in failing.stdout
            or "test result: FAILED. 0 passed; 1 failed; 0 ignored;" not in failing.stdout):
        raise ValueError(f"removed catalog family did not fail the unchanged case: {failing}")
    return cases, sha(lock), sha(mutant_lock), removed


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for key in ("framework", "compiler_target", "original", "original_target", "receipt"):
        ap.add_argument(key, type=lambda text: Path(text).resolve())
    ap.add_argument("--framework-revision", required=True)
    args = ap.parse_args()
    for repo, revision in [(args.framework, args.framework_revision), (args.original, ORIGINAL)]:
        if run("git", "rev-parse", "HEAD", cwd=repo).decode().strip() != revision or run(
                "git", "status", "--porcelain", cwd=repo):
            raise ValueError(f"expected clean pinned checkout {revision}")
    root = HERE.parents[2]
    if run("git", "status", "--porcelain", cwd=root):
        raise ValueError("expected clean companion checkout")
    compose(args.original, check=True)
    version = run("rustc", "+1.97.1", "-vV").decode()
    host, = (line[6:] for line in version.splitlines() if line.startswith("host: "))
    run("cargo", "+1.96.1", "build", "--offline", "--locked", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework)
    probe = (HERE / "observe.rs").read_bytes()
    with tempfile.TemporaryDirectory(prefix="odd-catalog-reference-", dir="/tmp") as name:
        scratch = Path(name).resolve()
        args.compiler = scratch / "oddlang"
        shutil.copy2(args.compiler_target / "debug/oddlang", args.compiler)
        helper, body, expected = reference(args.original, args.original_target, host, scratch, probe)
        with tempfile.TemporaryDirectory(prefix="odd-catalog-sealed-") as name:
            results, lock, mutant, removed = generated(args, host, Path(name).resolve(), scratch, helper, body, probe)
        if results != expected:
            raise ValueError("generated catalog guard observations differ from pinned original")
        FILE["write"](args.receipt, {
            "format": "marmish-local-verified-file-catalog-v1", "original": ORIGINAL,
            "framework": args.framework_revision, "compilerSha256": sha(args.compiler.read_bytes()),
            "companion": run("git", "rev-parse", "HEAD", cwd=root).decode().strip(),
            "rustc": version.splitlines(), "workspaceLockSha256": lock, "sourceDenials": 3,
            "case": "native_manifest::tests::" + CASE, "caseSha256": TEST_SHA,
            "sampleSha256": SAMPLE_SHA, "namedPassed": 1, "probeSha256": sha(probe),
            "observations": results, "mutation": {"removedPrefix": removed, "workspaceLockSha256": mutant,
                                                   "namedFailed": 1, "failure": "missed-ambiguity"},
        })


if __name__ == "__main__":
    main()
