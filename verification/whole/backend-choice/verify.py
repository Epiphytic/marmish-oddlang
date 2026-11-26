#!/usr/bin/env python3
"""Compare the pure public enum in original Rust and a sealed portable package."""
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
SOURCE = "crates/marmish-native/src/identity.rs"
HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang" / "portable"
COMPONENT = "native-backend-choice-v1"
MANIFEST = "native-backend-choice-package.json"

ABSENT_TRAITS = {
    "Debug": "std::fmt::Debug",
    "Display": "std::fmt::Display",
    "Default": "Default",
    "Hash": "std::hash::Hash",
    "Ord": "Ord",
    "PartialOrd": "PartialOrd",
    "Serialize": "serde::Serialize",
    "Deserialize": "for<'de> serde::Deserialize<'de>",
}


def run(*args, cwd=None, env=None):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, cwd=cwd, env=env).stdout


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def put(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def pin(base, path):
    return {"path": str(path.relative_to(base)), "sha256": sha(path)}


def compose(base):
    graph_dir = base / COMPONENT
    manifest = {
        "format": "oddlang-native-package-v1",
        "name": "marmish-native",
        "version": "0.3.1",
        "edition": "2024",
        "cargoLock": pin(base, graph_dir / "Cargo.lock"),
        "components": [{
            "kind": "checked-values", "module": "identity", "visibility": "public",
            "graph": pin(base, graph_dir / "graph.json"),
            "overlay": pin(base, graph_dir / "rust.json"),
            "reexports": ["BackendChoice"], "rootReexports": [],
        }],
    }
    path = base / MANIFEST
    put(path, manifest)
    return path


def build(package, target, snapshot, original):
    target.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["MARMISH_BUILD_COMMIT"] = PIN
    env.pop("CARGO_BUILD_TARGET", None)
    with (target / ".backend-choice.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX)
        lines = run("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "marmish-native",
                    "--lib", "--message-format=json", "--target-dir", target, cwd=package, env=env)
        selected = {}
        for line in lines.splitlines():
            row = json.loads(line)
            if row["reason"] != "compiler-artifact":
                continue
            crate = row["target"]["name"]
            if crate in ("serde", "marmish_native"):
                libraries = [Path(path) for path in row["filenames"] if path.endswith(".rlib")]
                if libraries:
                    assert crate not in selected
                    selected[crate], = libraries
        assert set(selected) == ({"serde", "marmish_native"} if original else {"marmish_native"})
        snapshot.mkdir()
        native = snapshot / "libmarmish_native.rlib"
        shutil.copy2(selected["marmish_native"], native)
    return (native, target / "debug" / "deps", selected.get("serde"))


def observe(native, deps, serde, serde_deps, scratch):
    scratch.mkdir()
    base_args = ["rustc", "+1.97.1", "--edition=2024", "-L", f"dependency={deps}",
                 "-L", f"dependency={serde_deps}", "--extern", f"marmish_native={native}",
                 "--extern", f"serde={serde}"]
    binary = scratch / "probe"
    run(*base_args, HERE / "probe.rs", "-o", binary)
    output = run(binary)

    checks = {
        **{trait: ("E0277", "use marmish_native::identity::BackendChoice;\n"
                   f"fn require<T: {bound}>() {{}}\n"
                   "fn main() { require::<BackendChoice>(); }")
           for trait, bound in ABSENT_TRAITS.items()},
        "integerConversion": ("E0277", "use marmish_native::identity::BackendChoice;\n"
                              "fn require<T: From<BackendChoice>>() {}\n"
                              "fn main() { require::<u8>(); }"),
        "crossTypeEquality": ("E0277", "use marmish_native::identity::BackendChoice;\n"
                              "fn require<T: PartialEq<BackendChoice>>() {}\n"
                              "fn main() { require::<u8>(); }"),
        "incompleteMatch": ("E0004", "use marmish_native::identity::BackendChoice;\n"
                            "fn main() { match BackendChoice::File { BackendChoice::Auto => (), "
                            "BackendChoice::Keychain => () } }"),
        "rootExposure": ("E0432", "use marmish_native::BackendChoice;\nfn main() {}"),
    }
    verified = {}
    for name, (code, text) in checks.items():
        path = scratch / "negative.rs"
        path.write_text(text + "\n")
        result = subprocess.run([*base_args, "--error-format=json", path, "-o", binary],
                                capture_output=True, text=True)
        errors = [json.loads(line) for line in result.stderr.splitlines()]
        actual = sorted({item["code"]["code"] for item in errors if item.get("code")})
        assert result.returncode != 0 and actual == [code], (name, result.stderr)
        verified[name] = code
    return output, verified


def recovered(cli, source, scratch):
    config = json.loads((HERE / "selection.json").read_bytes())
    config["sourceSha256"] = sha(source)
    selection = scratch / "selection.json"
    put(selection, config)
    out = scratch / "extracted"
    run(cli, "recover", "plain-choice", source, selection, out)
    return out, selection


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--original-target", required=True, type=Path)
    parser.add_argument("--generated-target", required=True, type=Path)
    args = parser.parse_args()
    original = args.original.resolve()
    cli = args.oddlang.resolve()
    assert run("git", "-C", original, "rev-parse", "HEAD").decode().strip() == PIN
    assert not run("git", "-C", original, "status", "--porcelain", "--untracked-files=no")
    assert args.original_target.resolve() != args.generated_target.resolve()
    args.output.mkdir()
    receipt = {
        "format": "marmish-native-backend-choice-evidence-v1", "originalRevision": PIN,
        "frameworkRevision": args.framework_revision,
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "probeSha256": sha(HERE / "probe.rs"), "lanes": {},
    }
    with tempfile.TemporaryDirectory(prefix="mbc-", dir="/tmp") as temp:
        root = Path(temp)
        mutation = root / "source-mutant"
        run("git", "clone", "--shared", "--no-checkout", original, mutation)
        run("git", "-C", mutation, "checkout", "--detach", PIN)
        source = mutation / SOURCE
        text = source.read_bytes()
        first = b"pub enum BackendChoice {\n    Auto,\n    Keychain,\n    File,\n}"
        replacement = b"pub enum BackendChoice {\n    File,\n    Auto,\n    Keychain,\n}"
        assert text.count(first) == 1
        source.write_bytes(text.replace(first, replacement))
        env = os.environ.copy()
        for role in ("AUTHOR", "COMMITTER"):
            env[f"GIT_{role}_NAME"] = "Qualification Fixture"
            env[f"GIT_{role}_EMAIL"] = "fixture@example.invalid"
            env[f"GIT_{role}_DATE"] = "2026-11-26T12:00:00Z"
        run("git", "-C", mutation, "-c", "commit.gpgSign=false", "commit", "--only", "-m",
            "Rotate native choice declarations", "--", SOURCE, env=env)
        assert run("git", "-C", mutation, "diff", "--name-only", PIN).decode().splitlines() == [SOURCE]
        for lane, upstream in (("baseline", original), ("mutant", mutation)):
            work = root / lane
            work.mkdir()
            out, selected = recovered(cli, upstream / SOURCE, work)
            portable = work / "portable"
            assets = portable / COMPONENT
            assets.mkdir(parents=True)
            shutil.copyfile(PORTABLE / COMPONENT / "Cargo.lock", assets / "Cargo.lock")
            for name in ("graph.json", "rust.json"):
                shutil.copyfile(out / name, assets / name)
            spec = compose(portable)
            if lane == "baseline":
                for name in ("graph.json", "rust.json"):
                    assert (assets / name).read_bytes() == (PORTABLE / COMPONENT / name).read_bytes()
                assert spec.read_bytes() == (PORTABLE / MANIFEST).read_bytes()
                assert selected.read_bytes() == (HERE / "selection.json").read_bytes()
            generated = work / "generated"
            run(cli, "build", "native-package", "rust", spec, generated)
            upstream_lib, libs, serde = build(
                upstream, args.original_target.resolve() / lane, work / "original-lib", True)
            generated_lib, gen_libs, _ = build(
                generated, args.generated_target.resolve() / lane, work / "generated-lib", False)
            baseline = observe(upstream_lib, libs, serde, libs, work / "observe-original")
            candidate = observe(generated_lib, gen_libs, serde, libs, work / "observe-generated")
            assert baseline == candidate
            output, negatives = baseline
            ordinals = (0, 1, 2) if lane == "baseline" else (1, 2, 0)
            expected = [f"{case}|{ordinal}|{ordinal}|{ordinal}" for case, ordinal
                        in zip(("Auto", "Keychain", "File"), ordinals)]
            assert output.decode().splitlines() == expected, (lane, output)
            (args.output / (lane + ".txt")).write_bytes(output)
            receipt["lanes"][lane] = {
                "sourceRevision": run("git", "-C", upstream, "rev-parse", "HEAD").decode().strip(),
                "recovery": json.loads((out / "observation.json").read_bytes()),
                "assembledLockSha256": sha(generated / "package.lock"),
                "outputSha256": sha(args.output / (lane + ".txt")),
                "negativeRustDiagnostics": negatives,
            }
    assert not run("git", "-C", original, "status", "--porcelain", "--untracked-files=no")
    put(args.output / "receipt.json", receipt)


if __name__ == "__main__":
    main()
