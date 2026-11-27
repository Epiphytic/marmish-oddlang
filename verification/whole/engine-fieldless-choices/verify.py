"""Compare unchanged external data callers with full original and generated engine libraries."""
import argparse
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
DOMAIN = "engine-fieldless-choices-v1"
BASE = "engine-observational-records-package.json"
PACKAGE = "engine-fieldless-choices-package.json"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = Path("crates/marmish-engine/src/model.rs")
TYPES = ("Stream", "SubmitDisposition")

FRAMEWORK = "8e3320aacb0106d543c3e21ab1faf60a02132c04"
RUSTC = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HOSTS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")
SHARED = runpy.run_path(str(HERE.parent / "vault-mode/verify.py"))
WORKSPACE, JOURNAL, run = (SHARED[key] for key in ("WORKSPACE", "JOURNAL", "run"))


sha = JOURNAL["sha"]


def checkout(path, revision):
    SHARED["reject_local_cargo_config"](path)
    if (run("git", "rev-parse", "HEAD", cwd=path).decode().strip() != revision
            or run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)):
        raise ValueError(f"expected a clean checkout at {revision}: {path}")


def metadata(raw):
    rows = [json.loads(line) for line in raw.splitlines()]
    if rows[-1] != {"reason": "build-finished", "success": True}:
        raise ValueError("incomplete Cargo artifact record")
    return rows


def library(rows, manifest, target):
    library, = (
        Path(file).resolve() for row in rows if row["reason"] == "compiler-artifact"
        and Path(row["manifest_path"]).resolve() == manifest.resolve()
        and row["target"]["kind"] == ["lib"] and row["target"]["name"] == "marmish_engine"
        and row["profile"]["opt_level"] == "3" and not row["profile"]["test"]
        for file in row["filenames"] if file.endswith(".rlib")
    )
    if not library.is_relative_to(target) or not library.is_file():
        raise ValueError("unexpected native library location")
    return library


def reference(checkout, env, host, target):
    rows = metadata(run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--lib",
                        "--target", host, "--target-dir", target, "-p", "marmish-engine",
                        "--message-format=json", cwd=checkout, env=env))
    target_release = target / host / "release"
    serde, = (
        Path(file).resolve() for row in rows if row["reason"] == "compiler-artifact"
        and row["target"]["name"] == "serde" and row["target"]["kind"] == ["lib"]
        for file in row["filenames"] if file.endswith(".rlib") and Path(file).is_relative_to(target_release)
    )
    links = {link for row in rows if row["reason"] == "build-script-executed" for link in row["linked_paths"]}
    return library(rows, checkout / "crates/marmish-engine/Cargo.toml", target), serde, sorted(links)


def observe(library, serde, host_deps, links, host, work, copy_disposition, env):
    work.mkdir()
    options = ["rustc", "+1.97.1", "--target", host, "--edition=2024",
               "--extern", f"marmish_engine={library}", "--extern", f"serde={serde}"]
    dependencies = dict.fromkeys((library.parent, library.parent / "deps", serde.parent, host_deps))
    options += [part for path in dependencies for part in ("-L", f"dependency={path}")]
    options += [part for path in links for part in ("-L", path)]
    binary = work / "probe"
    run(*options, HERE / "probe.rs", "-o", binary, env=env)
    observed = run(binary).decode().splitlines()
    bounds = {"Copy": "Copy", "Default": "Default", "Ord": "Ord", "PartialOrd": "PartialOrd",
              "Hash": "std::hash::Hash", "Display": "std::fmt::Display",
              "Serialize": "serde::Serialize", "Deserialize": "for<'de> serde::Deserialize<'de>"}
    checks = {
        f"{kind}.{trait}": (
            None if trait == "Copy" and (kind == "Stream" or copy_disposition) else "E0277",
            f"fn require<T: {bound}>() {{}} fn main() {{ require::<marmish_engine::{kind}>(); }}",
        )
        for kind in TYPES for trait, bound in bounds.items()
    }
    checks["privateModule"] = ("E0603", "use marmish_engine::model::Stream; fn main() {}")
    verified = {}
    for label, (expected, source) in checks.items():
        attempt = subprocess.run([*options, "--emit=metadata", "--error-format=json", "-", "-o", work / "check.rmeta"],
                                 input=source.encode(), capture_output=True, env=env, timeout=60)
        codes = [row["code"]["code"] for line in attempt.stderr.splitlines()
                 if (row := json.loads(line)).get("code") and row["level"] == "error"]
        if (attempt.returncode == 0) != (expected is None) or codes != ([expected] if expected else []):
            raise ValueError(f"unexpected public trait or visibility result for {label}: {attempt.stderr!r}")
        verified[label] = expected or "available"
    return {"values": observed, "traits": verified}


def qualify(args, host, work, scratch, compiler, env):
    target = scratch / "input-tool-target"
    run("cargo", "+1.97.1", "build", "--quiet", "--locked", "--offline", "--manifest-path",
        HERE.parent / "canonical-public-data/Cargo.toml", "--bin", "engine-choice-inputs",
        "--target-dir", target, env=env)
    tool = target / "debug/engine-choice-inputs"
    run(tool, "compose", compiler, args.original)
    staged = Path(shutil.copy2(compiler, work / "oddlang"))
    sandbox, build_env = JOURNAL["sandbox"](WORKSPACE, work,
        [ROOT, args.original, args.framework, args.original_target, scratch], None)
    for path in (args.original / SOURCE, HERE / "probe.rs", args.framework / "src/lib.rs"):
        denied = subprocess.run([*sandbox, "/bin/cat", path], cwd=work, env=build_env, capture_output=True)
        if denied.returncode != 1 or denied.stdout or not any(
                text in denied.stderr for text in (b"Operation not permitted", b"No such file")):
            raise ValueError(f"sandbox failed to deny {path}")

    with (scratch / "original.tar").open("wb") as archive:
        subprocess.run(["git", "-C", args.original, "archive", ORIGINAL], stdout=archive, check=True)
    mutant = scratch / "mutated"
    mutant.mkdir()
    run("tar", "-xf", scratch / "original.tar", "-C", mutant)
    (mutant / SOURCE).write_bytes(run(tool, "mutate", args.original))
    results = {}
    for case, reference_source in (("baseline", args.original), ("mutation", mutant)):
        copy_disposition = case == "mutation"
        model = (reference_source / SOURCE).read_bytes()
        inputs = work / (case + "-inputs")
        declaration = json.loads(run(tool, "prepare", compiler, reference_source / SOURCE, inputs))
        graph = (inputs / DOMAIN / "graph.json").read_bytes()
        binding = (inputs / DOMAIN / "rust.json").read_bytes()
        spec = inputs / PACKAGE
        product = work / (case + "-engine")
        run(*sandbox, staged, "build", "native-package", "rust", spec, product, cwd=work, env=build_env)
        lock = (product / "package.lock").read_bytes()
        if case == "baseline" and lock != (PORTABLE / DOMAIN / "package.lock").read_bytes():
            raise ValueError("baseline package differs from its published generated lock")
        target = work / (case + "-target")
        built = metadata(run(*sandbox, "cargo", "build", "--release", "--locked", "--offline", "--lib",
                             "--manifest-path", product / "Cargo.toml", "--target-dir", target,
                             "--message-format=json", cwd=work, env=build_env))
        generated = library(built, product / "Cargo.toml", target)
        reference_target = args.original_target / case
        original, serde, links = reference(reference_source, env, host, reference_target)
        host_deps = reference_target / "release/deps"
        expected = observe(original, serde, host_deps, links, host, scratch / (case + "-original"), copy_disposition, env)
        actual = observe(generated, serde, host_deps, [], host, scratch / (case + "-generated"), copy_disposition, env)
        names = (("Stream", "Stdout"), ("Stream", "Stderr"),
                 ("SubmitDisposition", "Accepted"), ("SubmitDisposition", "Existing"))
        ordinals = [1, 0, 1, 0] if copy_disposition else [0, 1, 0, 1]
        expected_values = [f"{kind}:{name}:{name}:{ordinal}"
                           for (kind, name), ordinal in zip(names, ordinals)]
        if expected != actual or actual["values"] != expected_values:
            raise ValueError("native values do not reproduce the independent witnesses")
        results[case] = {"modelSha256": sha(model), "recovery": declaration, "graphSha256": sha(graph),
                         "bindingSha256": sha(binding), "enginePackageLockSha256": sha(lock),
                         "originalLibrarySha256": sha(original.read_bytes()),
                         "generatedLibrarySha256": sha(generated.read_bytes()), **actual}
    if results["baseline"]["graphSha256"] == results["mutation"]["graphSha256"]:
        raise ValueError("actual Rust model source mutation did not reach the graph")
    return results


def verify(args):
    for path, pin in ((ROOT, args.input_revision), (args.framework, FRAMEWORK), (args.original, ORIGINAL)):
        checkout(path, pin)
    if any(target.is_relative_to(path) for target in (args.compiler_target, args.original_target)
           for path in (ROOT, args.framework, args.original)):
        raise ValueError("expected external Cargo targets")
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    machine = {"Darwin": "arm64", "Linux": "aarch64"}.get(os.uname().sysname)
    if host not in HOSTS or os.uname().machine != machine or f"commit-hash: {RUSTC}" not in version:
        raise ValueError("requires the native pinned ARM64 Rust toolchain")
    with tempfile.TemporaryDirectory(prefix="engine-choices-source-") as temp, tempfile.TemporaryDirectory(
            prefix="engine-choices-isolated-") as area:
        scratch, work = Path(temp).resolve(), Path(area).resolve()
        env = SHARED["host_build_env"](scratch / "cargo-home")
        run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--bin", "oddlang",
            "--target-dir", args.compiler_target, cwd=args.framework, env=env)
        compiler = args.compiler_target / "release/oddlang"
        choices = qualify(args, host, work, scratch, compiler, env)
    for path, pin in ((ROOT, args.input_revision), (args.framework, FRAMEWORK), (args.original, ORIGINAL)):
        checkout(path, pin)
    return {"platform": host, "original": ORIGINAL, "inputRevision": args.input_revision,
            "framework": FRAMEWORK, "compilerSha256": sha(compiler.read_bytes()), "rustc": version,
            "probeSha256": sha((HERE / "probe.rs").read_bytes()), "verifierSha256": sha(Path(__file__).read_bytes()),
            "predecessorManifestSha256": sha((PORTABLE / BASE).read_bytes()), "scenarios": choices}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for name in ("framework", "original", "compiler_target", "original_target", "receipt"):
        parser.add_argument(name, type=lambda p: Path(p).resolve())
    args = parser.parse_args()
    result = verify(args)
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
