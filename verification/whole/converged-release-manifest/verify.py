"""Check the pure parser in the sealed seven-member public API workspace."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile

from compose import BASE, BASE_LOCK, BUNDLE, MEMBER, OUTPUT, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / (OUTPUT + ".json")
OBSERVATIONS = HERE.parent / "release-manifest"
PROJECTION = HERE.parent / "release-manifest-projection"
SOURCE = "crates/marmish-native/src/native_manifest.rs"
VECTOR_SOURCE = "tools/release-version-vectors.json"
CASES = ("strict_vectors", "shared_release_version_vectors")


def load(directory, name="verify.py"):
    previous = sys.modules.pop("compose", None)
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / name))
    finally:
        sys.path.pop(0)
        sys.modules.pop("compose", None)
        if previous is not None:
            sys.modules["compose"] = previous


CONVERGENCE = load(HERE.parent / "converged-public-api")
SHAPES = load(PROJECTION, "shapes.py")
DIFFERENTIAL = load(PROJECTION)
ENGINE, LIMIT, SIX = (CONVERGENCE[key] for key in ("ENGINE", "LIMIT", "SIX"))
run, sha, data = (SIX[key] for key in ("run", "sha", "data"))


def check(product):
    emitted = product / "workspace.lock"
    current = data(emitted)
    if emitted.read_bytes() != (PORTABLE / OUTPUT / "workspace.lock").read_bytes():
        raise ValueError("integrated workspace lock differs")
    previous = PORTABLE / BASE / "workspace.lock"
    if sha(previous) != BASE_LOCK:
        raise ValueError("unexpected seven-member predecessor")
    prior = data(previous)
    for key in ("format", "cargoLockSha256", "workspaceCargoManifestSha256"):
        if current[key] != prior[key]:
            raise ValueError(f"workspace predecessor differs: {key}")
    others = dict(current["members"])
    selected = others.pop(MEMBER)
    old_others = dict(prior["members"])
    old = old_others.pop(MEMBER)
    if len(others) != 6 or others != old_others or selected["dependencies"] != old["dependencies"] or (
        selected["workspaceCargoManifestSha256"] != old["workspaceCargoManifestSha256"]
    ):
        raise ValueError("all six sibling locks and the native Cargo manifest must be conserved")
    SIX["check_member_files"](product, current)
    # Remove the single new component from the observed lock and reconstruct
    # the predecessor's native wrapper and manifest digests.
    native = product / MEMBER
    leaf, _ = parser_leaf(native)
    group = leaf.parent.name
    prior_native = data(native / "package.lock")
    appended = prior_native["components"].pop()
    if appended["module"] != "native_manifest" or appended["kind"] != "line-grammar":
        raise ValueError("expected one appended native manifest component")
    declaration = f'#[path = "{group}/mod.rs"]\nmod native_manifest;\n'
    lib = (native / "src/lib.rs").read_text()
    if lib.count(declaration) != 1:
        raise ValueError("expected exactly one private wrapper declaration")
    prior_native["generatedFiles"] = {
        name: digest for name, digest in prior_native["generatedFiles"].items()
        if not name.startswith(f"src/{group}/")
    }
    prior_native["generatedFiles"]["src/lib.rs"] = hashlib.sha256(lib.replace(declaration, "").encode()).hexdigest()
    prior_native["manifestSha256"] = old["inputSha256"]
    encoded = json.dumps(prior_native, indent=2, sort_keys=True, ensure_ascii=False).encode()
    if hashlib.sha256(encoded).hexdigest() != old["standalonePackageLockSha256"]:
        raise ValueError("predecessor native projection or emitted byte pins changed")
    return current


def parser_leaf(native):
    lib = (native / "src/lib.rs").read_text()
    group, = re.findall(r'^#\[path = "(group_\d+)/mod\.rs"\]\nmod native_manifest;$', lib, re.M)
    group = native / "src" / group
    module = (group / "mod.rs").read_text()
    leaf, = re.findall(r"^mod (__projection_\d+);$", module, re.M)
    return group / (leaf + ".rs"), f"native_manifest::{leaf}"


def unchanged_tests(original):
    raw = run("git", "-C", original, "show", f'{SIX["ORIGINAL"]}:{SOURCE}')
    start = b"#[cfg(test)]\nmod tests {"
    stop = b"    #[test]\n    fn versioned_directory_discovery_supports_both_families_and_rejects_ambiguity()"
    if raw.count(start) != 1 or raw.count(stop) != 1:
        raise ValueError("unexpected upstream test selection")
    begin, end = raw.index(start), raw.index(stop)
    if end <= begin:
        raise ValueError("invalid upstream test coordinates")
    tests = raw[begin:end]
    names = tuple(name.decode() for name in re.findall(rb"    #\[test\]\n    fn ([a-z_]+)\(", tests))
    if names != CASES:
        raise ValueError("upstream selection does not contain exactly the pure parser tests")
    return tests + b"}\n"


def select_exact(product, name, env):
    output = run("cargo", "+1.97.1", "test", "--release", "--locked", "--offline",
                 "-p", MEMBER, "--lib", "--", "--exact", name, cwd=product, env=env).decode()
    if output.splitlines().count(f"test {name} ... ok") != 1:
        raise ValueError(f"missing native execution of {name}")


def private_import(build, caller, output, host):
    artifact = SIX["artifact"](build, "marmish_native")
    dependency = SIX["artifact"](build, "serde_json").parent
    rows = [json.loads(line) for line in build.splitlines()]
    native_metadata = [
        Path(path) for row in rows
        if row["reason"] == "compiler-artifact" and row["target"]["name"] == "marmish_native"
        for path in row["filenames"] if path.endswith(".rmeta")
    ]
    if native_metadata:
        artifact, = native_metadata
    linked = sorted({path for row in rows if row["reason"] == "build-script-executed"
                     for path in row["linked_paths"]})
    source = subprocess.run(
        ["rustc", "+1.97.1", "--edition=2024", "--target", host,
         "--crate-type=lib", "--emit=metadata", "--error-format=json", caller,
         "-L", f"dependency={dependency}", "-L", f"dependency={artifact.parent}",
         "--extern", f"marmish_native={artifact}", "-o", output,
         *(arg for path in linked for arg in ("-L", path))],
        capture_output=True, text=True, timeout=60)
    errors = [diag for line in source.stderr.splitlines() if line.startswith("{")
              and (diag := json.loads(line)).get("level") == "error" and diag.get("code") is not None]
    if not source.returncode or [error["code"]["code"] for error in errors] != ["E0603"]:
        raise ValueError(f"native parser is not private or the caller failed another way\n{source.stderr}")
    if "module `native_manifest` is private" not in errors[0]["message"]:
        raise ValueError(f"unrelated private module diagnostic: {errors[0]['message']}")
    return "E0603"


def manifest_oracle(args, host, scratch, product, build):
    native = product / MEMBER
    leaf, test_path = parser_leaf(native)
    lib = native / "src/lib.rs"
    original_source, original_lib = leaf.read_bytes(), lib.read_bytes()
    native_build_env = SIX["JOURNAL"]["cargo_env"]()
    # Use Cargo's sealed build artifacts before Rustdoc runs its check
    # profile through the same dependency cache.
    privacy = scratch / "outside.rs"
    privacy.write_text("pub use marmish_native::native_manifest::Manifest;\n")
    original_build = SIX["cargo_reference"](args, MEMBER)
    privacy_witness = {
        "original": private_import(original_build, privacy, scratch / "original.rmeta", host),
        "generated": private_import(build, privacy, scratch / "generated.rmeta", host),
    }
    generated_doc = DIFFERENTIAL["rustdoc"](product, product / "target", native_build_env)
    reference_doc = DIFFERENTIAL["rustdoc"](args.original, args.original_target, native_build_env)
    graph = data(PORTABLE / "release-manifest-v1/graph.json")
    binding = data(PORTABLE / BUNDLE / "rust.json")
    shape = SHAPES["shape"](generated_doc, leaf.relative_to(product).as_posix())
    if shape != SHAPES["shape"](reference_doc, SOURCE) or shape != SHAPES["declared"](graph, binding):
        raise ValueError("native library records changed when integrated")

    source_cases = unchanged_tests(args.original)
    official = data(OBSERVATIONS / f"{host}.json")
    vector_source = args.original / VECTOR_SOURCE
    if official["originalRevision"] != SIX["ORIGINAL"] or (
        official["sourcePins"][SOURCE] != sha(args.original / SOURCE)
        or official["sourcePins"][VECTOR_SOURCE] != sha(vector_source)
        or official["unchangedPureTests"] != [f"test native_manifest::tests::{case} ... ok" for case in CASES]
    ):
        raise ValueError("expected pinned original tests, vector include and observations")
    source_vectors = data(OBSERVATIONS / "vectors.json")
    input_file = scratch / "blind.json"
    input_file.write_text(json.dumps([{"case": row["case"], "hex": row["hex"]} for row in source_vectors]))
    result = scratch / "integrated.json"
    env = dict(native_build_env, MANIFEST_CASES=str(input_file), MANIFEST_RESULT=str(result))
    vector_target = product / VECTOR_SOURCE
    probe = native / "src/manifest_oracle.rs"
    if vector_target.exists() or probe.exists():
        raise ValueError("original test inputs must not exist in the generated package")
    vector_target.parent.mkdir(parents=True, exist_ok=True)
    try:
        vector_target.write_bytes(vector_source.read_bytes())
        leaf.write_bytes(original_source + b"\n" + source_cases)
        lib.write_bytes(original_lib + b'\n#[cfg(test)]\n#[path = "manifest_oracle.rs"]\nmod manifest_oracle;\n')
        probe.write_bytes((OBSERVATIONS / "probe.rs").read_bytes())
        for case in CASES:
            select_exact(product, f"{test_path}::tests::{case}", env)
        select_exact(product, "manifest_oracle::manifest_contract_probe", env)
    finally:
        vector_target.unlink(missing_ok=True)
        probe.unlink(missing_ok=True)
        leaf.write_bytes(original_source)
        lib.write_bytes(original_lib)
    rows = data(result)
    checker = load(OBSERVATIONS, "conformance.py")
    checker["check"](rows, checker["cases"]())
    if rows != official["baseline"]:
        raise ValueError("integrated parser differs from the actual original observation")
    standalone = data(PROJECTION / f"{host}.json")
    if sha(result) != standalone["lanes"]["baseline"]["resultsSha256"]:
        raise ValueError("integrated result stream differs from the sealed standalone release")
    return {
        "scope": "crate-private Manifest::parse, id and asset_name",
        "cases": len(rows),
        "accepted": sum("error" not in row["result"] for row in rows),
        "resultsSha256": sha(result),
        "originalOracleSha256": sha(OBSERVATIONS / f"{host}.json"),
        "standaloneReceiptSha256": sha(PROJECTION / f"{host}.json"),
        "nativeRecords": shape,
        "privateDownstreamImport": privacy_witness,
        "unchangedTestModuleSha256": hashlib.sha256(source_cases).hexdigest(),
        "originalVectorIncludeSha256": sha(vector_source),
        "unchangedPureTests": [f"{test_path}::tests::{case}" for case in CASES],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda p: Path(p).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda p: Path(p).resolve())
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--raw-results", type=Path)
    args = parser.parse_args()
    for checkout, revision in ((args.framework, args.framework_revision), (args.original, SIX["ORIGINAL"]),
                               (ROOT, None)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=checkout) or (
            revision and run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() != revision
        ):
            raise ValueError(f"expected clean pinned checkout {checkout}")
    host, = (line.removeprefix("host: ") for line in
             run("rustc", "+1.97.1", "-vV").decode().splitlines() if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError("unsupported native proof platform")
    compose(PORTABLE, check=True)
    with tempfile.TemporaryDirectory(prefix="odd-converged-manifest-") as staging, tempfile.TemporaryDirectory(
        prefix="odd-converged-observations-"
    ) as testing:
        work, scratch = (Path(path).resolve() for path in (staging, testing))

        def observe(product, build):
            inherited = CONVERGENCE["observe"](args, host, work, scratch, product, build)
            inherited["releaseManifest"] = manifest_oracle(args, host, scratch, product, build)
            return inherited

        receipt = SIX["qualify"](
            args, host, work, scratch, manifest=MANIFEST, base=BASE + ".json", member=MEMBER, check=check,
            extra_denied={
                "original-prototype": args.original / LIMIT["SOURCE"],
                "constant-caller": LIMIT["FILE"],
                "reader-caller": LIMIT["BUFFERED"]["PRIVATE"] / "observations.rs",
                "original-engine-error": args.original / ENGINE["SOURCE"],
                "engine-error-caller": ENGINE["HERE"] / "probe.rs",
                "original-native-choice": args.original / ENGINE["IDENTITY"]["SOURCE"],
                "choice-caller": HERE.parent / "backend-choice/probe.rs",
                "original-release-manifest": args.original / SOURCE,
                "original-release-versions": args.original / VECTOR_SOURCE,
                "manifest-oracle-caller": OBSERVATIONS / "probe.rs",
                "manifest-blind-vectors": OBSERVATIONS / "vectors.json",
                "manifest-original-receipt": OBSERVATIONS / f"{host}.json",
            }, extra_artifact_oracle=observe,
            receipt_format="marmish-converged-pure-release-manifest-v1",
        )
        if args.raw_results is not None:
            args.raw_results.write_bytes((scratch / "integrated.json").read_bytes())
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
