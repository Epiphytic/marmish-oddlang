"""Build and replay the pure Rust release-manifest contract with source excluded."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

from shapes import declared, shape

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
INPUT = PORTABLE / "native-release-manifest-package.json"
OBSERVATIONS = WHOLE / "release-manifest"
SOURCE = "crates/marmish-native/src/native_manifest.rs"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
GENERATED = "src/group_1/__projection_1.rs"
PROBE = OBSERVATIONS / "probe.rs"
DEV = b'\n[dev-dependencies]\nserde_json = "=1.0.151"\nhex = "=0.4.3"\n'


def read(file):
    return json.loads(file.read_bytes())


def write(file, content):
    file.write_text(json.dumps(content, indent=2) + "\n")


def sha(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


def run(*argv, cwd=None, env=None):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, timeout=1200)
    if result.returncode:
        raise ValueError(f"{argv}\n{result.stdout.decode(errors='replace')}{result.stderr.decode(errors='replace')}")
    return result.stdout


def lane(path):
    sys.path.insert(0, str(path.parent))
    try:
        return runpy.run_path(str(path))
    finally:
        sys.path.pop(0)


def stage(dst, changed=None):
    manifest = read(INPUT)
    dst.mkdir()
    roles = manifest["components"]
    pins = [manifest["cargoLock"], *(component[key] for component in roles for key in ("graph", "overlay"))]
    for pin in pins:
        source = PORTABLE / pin["path"]
        if sha(source) != pin["sha256"]:
            raise ValueError(f"source-free closure pin differs: {source.name}")
        target = dst / pin["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    if changed is not None:
        line, = [c for c in roles if c["kind"] == "line-grammar"]
        graph = dst / line["graph"]["path"]
        write(graph, changed)
        line["graph"]["sha256"] = sha(graph)
    plan = dst / INPUT.name
    write(plan, manifest)
    return plan


def rustdoc(root, target, env):
    run("cargo", "+1.97.1", "rustdoc", "--release", "--locked", "--offline",
        "--target-dir", target, "-p", "marmish-native", "--lib", "--",
        "-Z", "unstable-options", "--output-format", "json", "--document-private-items",
        cwd=root, env=dict(env, RUSTC_BOOTSTRAP="1"))
    return target / "doc/marmish_native.json"


def record_native(root, product, graph, binding, original_target, scratch, env):
    original = rustdoc(root, original_target, env)
    generated = rustdoc(product, scratch / "rustdoc-generated", env)
    source_shape = shape(original, SOURCE)
    if source_shape != shape(generated, GENERATED) or source_shape != declared(graph, binding):
        raise ValueError("original/compiler-observed and projected native record fields differ")
    return {
        "fieldsAndTraits": source_shape,
        "originalRustdocSha256": sha(original),
        "generatedRustdocSha256": sha(generated),
    }


def invoke_probe(product, scratch, inputs, env):
    # Admit the independent native caller only after the sealed library is built.
    test = scratch / "test"
    shutil.copytree(product / "src", test / "src")
    (test / "Cargo.toml").write_bytes((product / "Cargo.toml").read_bytes() + DEV)
    shutil.copyfile(HERE / "observer.Cargo.lock", test / "Cargo.lock")
    lib = test / "src/lib.rs"
    lib.write_bytes(lib.read_bytes() + b'\n#[cfg(test)]\n#[path = "oracle.rs"]\nmod oracle;\n')
    (test / "src/oracle.rs").write_bytes(PROBE.read_bytes())
    result = scratch / "results.json"
    name = "oracle::manifest_contract_probe"
    output = run("cargo", "+1.97.1", "test", "--release", "--locked", "--offline",
                 "--target-dir", scratch.parent / "caller-target", "--lib", "--", "--exact", name,
                 cwd=test, env=dict(env, MANIFEST_CASES=str(inputs), MANIFEST_RESULT=str(result)))
    if output.decode().splitlines().count(f"test {name} ... ok") != 1:
        raise ValueError("native probe did not run")
    return read(result), sha(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    for name in ("compiler-target", "original-target"):
        parser.add_argument("--" + name, required=True, type=lambda value: Path(value).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for directory, revision in ((ROOT, None), (args.original, ORIGINAL), (args.framework, args.framework_revision)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=directory) or (
            revision and run("git", "rev-parse", "HEAD", cwd=directory).decode().strip() != revision
        ):
            raise ValueError(f"expected clean pinned checkout at {directory}")
    rustc = run("rustc", "+1.97.1", "-vV").decode()
    host, = [line.removeprefix("host: ") for line in rustc.splitlines() if line.startswith("host: ")]
    if host not in ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"):
        raise ValueError("no original witness staged for this host")
    reference_file = OBSERVATIONS / f"{host}.json"
    reference = read(reference_file)
    vectors = read(OBSERVATIONS / "vectors.json")
    checker = lane(OBSERVATIONS / "conformance.py")
    fixtures = checker["cases"]()
    checker["check"](reference["baseline"], fixtures)
    if reference["originalRevision"] != ORIGINAL or sha(args.original / SOURCE) != reference["sourcePins"][SOURCE]:
        raise ValueError("release parser source provenance changed")
    source = (args.original / SOURCE).read_bytes()
    selected = b"                    16 * 1024 * 1024\n"
    if source.count(selected) != 1:
        raise ValueError("expected one observed changed-source expression")
    mutation = source.replace(selected, b"                    16 * 1024 * 1024 + 1\n")
    if hashlib.sha256(mutation).hexdigest() != reference["mutatedSourceSha256"]:
        raise ValueError("pinned source mutation does not match the original observation")
    graph = read(PORTABLE / "release-manifest-v1/graph.json")
    binding = read(PORTABLE / "release-manifest-v1/rust.json")
    if graph["textGrammars"] != [read(PORTABLE / "release-version-grammar-v1/graph.json")]:
        raise ValueError("linked release version grammar changed")
    changed = read(PORTABLE / "release-manifest-v1/graph.json")
    license_cap, = [t for row in changed["rows"] for t in row if t.get("capture") == "file-4-size"]
    if license_cap["maximum"] != 16 * 1024 * 1024:
        raise ValueError("expected the pinned license bound before applying the source-mutant policy")
    license_cap["maximum"] += 1
    original_delta = {row["case"]: row["result"] for row in reference["mutation"]}
    if set(original_delta) != {"size-3-16777217"}:
        raise ValueError("source mutation must have exactly the named native delta")
    expected_change = {
        name: (bytes_, original_delta.get(name, outcome)) for name, (bytes_, outcome) in fixtures.items()
    }
    journal = lane(WHOLE / "journal-records/verify.py")
    assembly = lane(WHOLE / "workspace/verify.py")
    env = journal["cargo_env"]()
    run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang", "--target-dir",
        args.compiler_target, cwd=args.framework, env=env)
    with tempfile.TemporaryDirectory(prefix="odd-manifest-inputs-") as temporary, tempfile.TemporaryDirectory(
        prefix="odd-manifest-oracles-"
    ) as checks:
        work, scratch = (Path(path).resolve() for path in (temporary, checks))
        compiler = work / "oddlang"
        shutil.copy2(args.compiler_target / "debug/oddlang", compiler)
        plans = {tag: stage(work / tag, value) for tag, value in (("baseline", None), ("changed", changed))}
        prefix, isolated = journal["sandbox"](assembly, work, (ROOT, args.framework, args.original,
                                                args.original_target, args.compiler_target, scratch), None)

        def isolate(*argv, cwd=work):
            command = list(prefix)
            if sys.platform == "linux":
                command[command.index("--chdir") + 1] = str(cwd)
            return [*command, *argv]

        denied = {
            "original-parser": args.original / SOURCE,
            "oracle-caller": PROBE,
            "original-observations": reference_file,
            "blind-vectors": OBSERVATIONS / "vectors.json",
            "compiler-source": args.framework / "src/lib.rs",
        }
        for name, file in denied.items():
            if not file.is_file():
                raise ValueError(f"missing denial probe {name}")
            p = subprocess.run(isolate("/bin/cat", file), cwd=work, env=isolated, capture_output=True)
            if p.returncode != 1 or p.stdout or not any(
                reason in p.stderr for reason in (b"Operation not permitted", b"No such file or directory")
            ):
                raise ValueError(f"generator could read {name}")

        products = {}
        for tag, plan in plans.items():
            product = work / (tag + "-product")
            run(*isolate(compiler, "build", "native-package", "rust", plan, product), cwd=work, env=isolated)
            run(*isolate("cargo", "build", "--release", "--locked", "--offline", cwd=product),
                cwd=product, env=isolated)
            products[tag] = product
        frozen = PORTABLE / "release-manifest-v1/package.lock"
        if (products["baseline"] / "package.lock").read_bytes() != frozen.read_bytes():
            raise ValueError("baseline projection lock differs from release pin")
        shapes = record_native(args.original, products["baseline"], graph, binding,
                               args.original_target, scratch, env)
        inputs = scratch / "inputs.json"
        write(inputs, [{"case": row["case"], "hex": row["hex"]} for row in vectors])
        results = {}
        for tag, expected in (("baseline", fixtures), ("changed", expected_change)):
            lane_dir = scratch / tag
            lane_dir.mkdir()
            rows, raw_hash = invoke_probe(products[tag], lane_dir, inputs, env)
            checker["check"](rows, expected)
            reference_rows = (reference["baseline"] if tag == "baseline" else
                              [{"case": row["case"], "result": original_delta.get(row["case"], row["result"])}
                               for row in reference["baseline"]])
            if rows != reference_rows:
                raise ValueError(f"original and generated native rows disagree in {tag}")
            results[tag] = {"cases": len(rows), "accepted": sum("error" not in row["result"] for row in rows),
                            "resultsSha256": raw_hash, "packageLockSha256": sha(products[tag] / "package.lock"),
                            "projectionSha256": sha(products[tag] / GENERATED)}
        reference_delta = sorted(original_delta)
        receipt = {
            "format": "marmish-pure-release-manifest-projection-v1",
            "original": ORIGINAL,
            "framework": args.framework_revision,
            "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
            "platform": host,
            "rustc": rustc.splitlines(),
            "compilerSha256": sha(compiler),
            "sourceSha256": reference["sourcePins"][SOURCE],
            "changedSourceSha256": reference["mutatedSourceSha256"],
            "oracleReceiptSha256": sha(reference_file),
            "vectorsSha256": sha(OBSERVATIONS / "vectors.json"),
            "callerSha256": sha(PROBE),
            "observerCargoLockSha256": sha(HERE / "observer.Cargo.lock"),
            "deniedSourceProbes": sorted(denied),
            "resultShapes": shapes,
            "changedCases": reference_delta,
            "lanes": results,
        }
    write(args.receipt, receipt)


if __name__ == "__main__":
    main()
