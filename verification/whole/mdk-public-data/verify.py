#!/usr/bin/env python3
"""Qualify the pure public MDK error and record with native Rust."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PIN = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = "crates/marmish-mdk/src/lib.rs"
HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang" / "portable"
INPUTS = "mdk-public-data-v1"
MANIFEST = "mdk-public-data-package.json"
PROFILES = (("finite-error-domain", "error", "AdapterError"),
            ("public-root-records", "record", "VersionObservation"))


def run(*args, cwd=None, env=None):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, cwd=cwd, env=env).stdout


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def put(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def pin(base, path):
    return {"path": str(path.relative_to(base)), "sha256": sha(path)}


def compose(base):
    directory = base / INPUTS
    rows = []
    for kind, role, symbol in PROFILES:
        rows.append({
            "kind": "public-records" if kind == "public-root-records" else kind,
            "module": f"data_{role}", "visibility": "private", "reexports": [symbol],
            "rootReexports": [symbol], "graph": pin(base, directory / f"{role}.json"),
            "overlay": pin(base, directory / f"{role}-rust.json"),
        })
    package = base / MANIFEST
    put(package, {"format": "oddlang-native-package-v1", "name": "marmish-mdk",
                  "version": "0.1.0", "edition": "2021",
                  "cargoLock": pin(base, directory / "Cargo.lock"), "components": rows})
    return package


def recover(cli, source, work, inputs):
    facts = {}
    for kind, role, _ in PROFILES:
        chosen = HERE / (role + "-selection.json")
        data = json.loads(chosen.read_bytes())
        data["sourceSha256"] = sha(source)
        selection = work / chosen.name
        put(selection, data)
        out = work / ("extracted-" + role)
        run(cli, "recover", kind, source, selection, out)
        for leaf, suffix in (("graph.json", ".json"), ("rust.json", "-rust.json")):
            shutil.copyfile(out / leaf, inputs / (role + suffix))
        lock = "observation.json" if role == "error" else "recovery.lock"
        facts[role] = json.loads((out / lock).read_bytes())
    return facts


def build(workspace, cache, snapshot, original):
    cache.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop("CARGO_BUILD_TARGET", None)
    with (cache / ".mdk-public-data.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX)
        cargo = run("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "marmish-mdk",
                    "--lib", "--message-format=json", "--target-dir", cache, cwd=workspace, env=env)
        found = {}
        for line in cargo.splitlines():
            message = json.loads(line)
            if message["reason"] != "compiler-artifact" or message["target"]["name"] not in ("marmish_mdk", "serde"):
                continue
            name = message["target"]["name"]
            paths = [path for path in message["filenames"] if path.endswith(".rlib")]
            if paths:
                assert name not in found
                found[name], = map(Path, paths)
        assert set(found) == ({"marmish_mdk", "serde"} if original else {"marmish_mdk"})
        snapshot.mkdir()
        library = snapshot / "libmarmish_mdk.rlib"
        shutil.copy2(found["marmish_mdk"], library)
    return library, cache / "debug" / "deps", found.get("serde")


def observe(native, deps, serde, serde_deps, work):
    work.mkdir()
    argv = ["rustc", "+1.97.1", "--edition=2021", "-L", f"dependency={deps}",
            "-L", f"dependency={serde_deps}", "--extern", f"marmish_mdk={native}",
            "--extern", f"serde={serde}"]
    binary = work / "probe"
    run(*argv, HERE / "probe.rs", "-o", binary)
    output = run(binary)
    cases = {"rootResult": ("E0432", "use marmish_mdk::Result;\nfn main() {}")}
    for name in ("VersionObservation", "AdapterError"):
        traits = {"Clone": "Clone", "Copy": "Copy", "Eq": "Eq", "PartialEq": "PartialEq",
                  "Default": "Default", "Serialize": "serde::Serialize",
                  "Deserialize": "for<'de> serde::Deserialize<'de>"}
        if name == "VersionObservation":
            traits["Error"] = "std::error::Error"
        for label, bound in traits.items():
            cases[f"{name}:{label}"] = ("E0277", f"use marmish_mdk::{name};\n"
                                      f"fn require<T: {bound}>() {{}}\n"
                                      f"fn main() {{ require::<{name}>(); }}")
    observed = {}
    for name, (expected, source) in cases.items():
        path = work / "negative.rs"
        path.write_text(source + "\n")
        compiled = subprocess.run([*argv, "--error-format=json", path, "-o", binary],
                                  capture_output=True, text=True)
        diagnostics = [json.loads(line) for line in compiled.stderr.splitlines()]
        codes = {entry["code"]["code"] for entry in diagnostics if entry.get("code")}
        assert compiled.returncode and codes == {expected}, (name, compiled.stderr)
        observed[name] = expected
    return output, observed


def mutate(original, destination):
    run("git", "clone", "--shared", "--no-checkout", original, destination)
    run("git", "-C", destination, "checkout", "--detach", PIN)
    path = destination / SOURCE
    data = path.read_bytes()
    changes = [
        (b'#[error("response is malformed or truncated")]', b'#[error("response was rejected")]'),
        (b"pub struct VersionObservation {\n    pub stdout: String,\n    pub stderr: String,\n}",
         b"pub struct VersionObservation {\n    pub stderr: String,\n    pub stdout: String,\n}"),
    ]
    for previous, changed in changes:
        assert data.count(previous) == 1
        data = data.replace(previous, changed)
    path.write_bytes(data)
    env = os.environ.copy()
    for role in ("AUTHOR", "COMMITTER"):
        env[f"GIT_{role}_NAME"] = "Qualification Fixture"
        env[f"GIT_{role}_EMAIL"] = "fixture@example.invalid"
        env[f"GIT_{role}_DATE"] = "2026-11-26T12:00:00Z"
    run("git", "-C", destination, "-c", "commit.gpgSign=false", "commit", "--only", "-m",
        "Change MDK data and error observations", "--", SOURCE, env=env)
    assert run("git", "-C", destination, "diff", "--name-only", PIN).decode().splitlines() == [SOURCE]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--original-target", required=True, type=Path)
    parser.add_argument("--generated-target", required=True, type=Path)
    args = parser.parse_args()
    original, cli = args.original.resolve(), args.oddlang.resolve()
    assert run("git", "-C", original, "rev-parse", "HEAD").decode().strip() == PIN
    assert not run("git", "-C", original, "status", "--porcelain", "--untracked-files=no")
    assert args.original_target.resolve() != args.generated_target.resolve()
    args.output.mkdir()
    receipt = {
        "format": "marmish-mdk-public-data-evidence-v1", "originalRevision": PIN,
        "frameworkRevision": args.framework_revision,
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "probeSha256": sha(HERE / "probe.rs"), "lanes": {},
    }
    with tempfile.TemporaryDirectory(prefix="mmd-", dir="/tmp") as scratch:
        root = Path(scratch)
        mutant = root / "source-mutant"
        mutate(original, mutant)
        for lane, reference in (("baseline", original), ("mutant", mutant)):
            work = root / lane
            portable = work / "portable"
            directory = portable / INPUTS
            directory.mkdir(parents=True)
            facts = recover(cli, reference / SOURCE, work, directory)
            shutil.copyfile(PORTABLE / INPUTS / "Cargo.lock", directory / "Cargo.lock")
            manifest = compose(portable)
            if lane == "baseline":
                assert manifest.read_bytes() == (PORTABLE / MANIFEST).read_bytes()
                for path in directory.iterdir():
                    assert path.read_bytes() == (PORTABLE / INPUTS / path.name).read_bytes()
            generated = work / "generated"
            run(cli, "build", "native-package", "rust", manifest, generated)
            upstream, deps, serde = build(reference, args.original_target.resolve() / lane,
                                          work / "original", True)
            compiled, target, _ = build(generated, args.generated_target.resolve() / lane,
                                        work / "compiled", False)
            observed = observe(upstream, deps, serde, deps, work / "observe-original")
            candidate = observe(compiled, target, serde, deps, work / "observe-generated")
            assert observed == candidate, lane
            output, negative = observed
            report = args.output / (lane + ".txt")
            report.write_bytes(output)
            receipt["lanes"][lane] = {
                "sourceRevision": run("git", "-C", reference, "rev-parse", "HEAD").decode().strip(),
                "recovery": facts, "packageLockSha256": sha(generated / "package.lock"),
                "outputSha256": sha(report), "negativeRustDiagnostics": negative,
            }
    before = (args.output / "baseline.txt").read_text().splitlines()
    after = (args.output / "mutant.txt").read_text().splitlines()
    assert len(before) == len(after) == 8
    assert [i for i, (left, right) in enumerate(zip(before, after)) if left != right] == [4, 7]
    assert before[4] == "InvalidResponse|InvalidResponse|response is malformed or truncated|4"
    assert after[4] == "InvalidResponse|InvalidResponse|response was rejected|4"
    assert before[7].startswith("VersionObservation { stdout:")
    assert after[7].startswith("VersionObservation { stderr:")
    assert not run("git", "-C", original, "status", "--porcelain", "--untracked-files=no")
    put(args.output / "receipt.json", receipt)


if __name__ == "__main__":
    main()
