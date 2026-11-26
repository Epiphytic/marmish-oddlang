#!/usr/bin/env python3
"""Differentially qualify the released pure version grammar and a source change."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
DATA = ROOT / "oddlang/portable/release-version-grammar-v1"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK = "c78a9b82ac0aa6148054a5c36eb519689223ef9d"
SOURCE = "crates/marmish-native/src/native_manifest.rs"
VECTORS = "tools/release-version-vectors.json"
NATIVE_TEST = "native_manifest::tests::strict_vectors"

# These expectations are literal witnesses independent of the extractor and projector.
# The first boolean is for the pinned original and the second is for the changed source.
BOUNDARIES = [
    ("0.3.1", True, False),
    ("0.4.1", False, True),
    ("0.2.4294967295", True, True),
    ("0.2.4294967296", True, False),
    ("0.2.18446744073709551615", True, False),
    ("0.2.1-rc.2", True, False),
    ("0.2.1-preview.2", False, True),
    ("0.2.1-preview.0", False, True),
    ("0.2.1-preview.01", False, False),
    ("0.2.1-preview.4294967295", False, True),
    ("0.2.1-preview.4294967296", False, False),
    ("0.2.1-preview.2tail", False, False),
    ("0.2.\0", False, False),
    ("0.2.١", False, False),
    ("0.4.1\n", False, False),
]


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True).stdout


def sha(value):
    return hashlib.sha256(value).hexdigest()


def from_git(original, name):
    return run("git", "-C", original, "show", f"{ORIGINAL}:{name}")


def edit_source(source):
    changes = [
        (b'strip_prefix("0.3.")', b'strip_prefix("0.4.")'),
        (b'split_once("-rc.")', b'split_once("-preview.")'),
        (b"Option<u64>", b"Option<u32>"),
        (b"parse::<u64>()", b"parse::<u32>()"),
        (b"decimal(rc, true)", b"decimal(rc, false)"),
    ]
    for predecessor, successor in changes:
        assert source.count(predecessor) == 1, predecessor
        source = source.replace(predecessor, successor)
    return source


def pure_functions(source):
    start = source.index(b"pub(crate) fn version(")
    end = source.index(b"pub(crate) fn hex32(", start)
    return source[start:end]


def observe(work, tag, implementation, entry, cases, prefix=()):
    source = work / f"{tag}.rs"
    executable = work / tag
    # Byte literals preserve arbitrary input text without adding Rust syntax from it.
    data = ",\n".join("&" + str(list(value.encode("utf-8"))) for value, _ in cases)
    source.write_bytes(
        b"mod predicate {\n" + implementation + b"\n}\n"
        + f"""fn main() {{
            let inputs: &[&[u8]] = &[{data}];
            for bytes in inputs {{
                let value = ::std::str::from_utf8(bytes).unwrap();
                println!("{{}}", u8::from(predicate::{entry}(value)));
            }}
        }}\n""".encode()
    )
    run(*prefix, "rustc", "+1.97.1", "--edition=2024", source, "-o", executable, cwd=work)
    text = run(*prefix, executable, cwd=work).decode().splitlines()
    assert text == ["1" if expected else "0" for _, expected in cases], (tag, text)
    return sha(("\n".join(text) + "\n").encode())


def sandbox(work, original, framework, private):
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    roots = (original, framework, ROOT, private, Path("/private/tmp"), Path.home() / "repos")
    assert not any(work.is_relative_to(root) for root in roots)
    profile = "(version 1)(allow default)(deny network*)"
    profile += "".join("(deny file-read* (subpath " + json.dumps(str(root)) + "))"
                       for root in roots)
    prefix = ["/usr/bin/sandbox-exec", "-p", profile, "/usr/bin/env", f"TMPDIR={work}"]
    probes = {"originalModule": original / SOURCE, "sourceControl": private / "mutated.rs"}
    for path in probes.values():
        assert path.is_file()
        got = subprocess.run([*prefix, "/bin/cat", path], capture_output=True)
        assert got.returncode and b"Operation not permitted" in got.stderr, path
    return prefix, sorted(probes)


def recover(work, cli, source, selection, label):
    private = work / "private"
    native = private / f"{label}.rs"
    native.write_bytes(source)
    descriptor = private / f"{label}.json"
    descriptor.write_bytes(selection)
    output = work / f"recovered-{label}"
    run(cli, "recover", "text-grammar", "rust", native, descriptor, output, cwd=work)
    return output


def check_shape(file, prefixes, width, delimiter, minimum):
    graph = json.loads(file.read_bytes())
    decimal = {"notation": "canonical-ascii-decimal", "bits": width}
    assert graph == {
        "format": "oddlang-text-grammar-v1",
        "id": "urn:marmish:native:release-version",
        "prefixes": prefixes,
        "body": dict(decimal, minimum=0),
        "suffix": {"delimiter": delimiter, "body": dict(decimal, minimum=minimum)},
        "consumption": "entire-input",
    }


def qualify(work, original, framework, compiler):
    private = work / "private"
    private.mkdir()
    cli = work / "oddlang"
    shutil.copyfile(compiler, cli)
    cli.chmod(0o755)
    source = from_git(original, SOURCE)
    native_vectors = from_git(original, VECTORS)
    vectors = json.loads(native_vectors)
    selection = (DATA / "selection.json").read_bytes()
    baseline = recover(work, cli, source, selection, "original")
    for name in ("graph.json", "recovery.lock"):
        assert (baseline / name).read_bytes() == (DATA / name).read_bytes()
    check_shape(baseline / "graph.json", ["0.2.", "0.3."], 64, "-rc.", 1)

    mutation = edit_source(source)
    changed_selection = json.dumps(dict(json.loads(selection), sourceSha256=sha(mutation))).encode()
    changed = recover(work, cli, mutation, changed_selection, "mutated")
    check_shape(changed / "graph.json", ["0.2.", "0.4."], 32, "-preview.", 0)
    prefix, denied = sandbox(work, original, framework, private)

    staged = work / "graphs"
    staged.mkdir()
    lanes = {}
    for name, graph, implementation, cases in [
        ("baseline", baseline, source,
         [(value, accepted) for key, accepted in (("valid", True), ("invalid", False))
          for value in vectors[key]] + [(value, before) for value, before, _ in BOUNDARIES]),
        ("mutation", changed, mutation, [(value, after) for value, _, after in BOUNDARIES]),
    ]:
        input_file = staged / f"{name}.json"
        shutil.copyfile(graph / "graph.json", input_file)
        projection = work / f"projection-{name}"
        run(*prefix, cli, "build", "text-grammar", "rust", input_file, projection, cwd=work)
        source_file = projection / "text.rs"
        native = observe(private, f"native-{name}", pure_functions(implementation), "version", cases)
        generated = observe(work, f"generated-{name}", source_file.read_bytes(), "accepts", cases, prefix)
        assert native == generated
        lanes[name] = {
            "sourceSha256": sha(implementation),
            "graphSha256": sha(input_file.read_bytes()),
            "projectionLockSha256": sha((projection / "projection.lock").read_bytes()),
            "sourceOfNativeWitnessSha256": sha(pure_functions(implementation)),
            "matchingCases": len(cases),
            "outcomesSha256": generated,
        }

    return {
        "frameworkCommit": FRAMEWORK,
        "originalCommit": ORIGINAL,
        "companionRevision": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "platform": "aarch64-apple-darwin",
        "deniedReadProbes": denied,
        "sourceSelectionSha256": sha(selection),
        "unmodifiedVectorsSha256": sha(native_vectors),
        "unmodifiedVectors": len(vectors["valid"]) + len(vectors["invalid"]),
        "changedBoundaryOutcomes": sum(before != after for _, before, after in BOUNDARIES),
        "verifierSha256": sha(Path(__file__).read_bytes()),
        "lanes": lanes,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "framework", "compiler_target", "reference_target", "receipt"):
        parser.add_argument(name, type=lambda item: Path(item).resolve())
    args = parser.parse_args()
    for checkout, revision in ((args.original, ORIGINAL), (args.framework, FRAMEWORK)):
        assert run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() == revision
        assert not run("git", "status", "--porcelain", "--untracked-files=no", cwd=checkout)
    assert not run("git", "status", "--porcelain", "--untracked-files=all", "--",
                   "oddlang/portable", "verification/whole", cwd=ROOT)
    run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--locked", "--offline",
        "--target-dir", args.compiler_target, cwd=args.framework)
    native_test = run("cargo", "+1.97.1", "test", "--locked", "--offline", "--release",
                      "--target", "aarch64-apple-darwin", "--target-dir", args.reference_target,
                      "--package", "marmish-native", "--lib", "--", "--exact", NATIVE_TEST,
                      cwd=args.original)
    assert f"test {NATIVE_TEST} ... ok".encode() in native_test
    assert b"test result: ok. 1 passed; 0 failed; 0 ignored;" in native_test
    parent = run("/usr/bin/getconf", "DARWIN_USER_TEMP_DIR").decode().strip()
    with tempfile.TemporaryDirectory(prefix="odd-release-version-grammar-", dir=parent) as directory:
        receipt = qualify(Path(directory).resolve(), args.original, args.framework,
                          args.compiler_target / "debug/oddlang")
    receipt["nativeUpstreamHarness"] = {"name": NATIVE_TEST, "passed": 1}
    receipt["rustc"] = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.stderr.decode(errors="replace") or str(error)) from error
