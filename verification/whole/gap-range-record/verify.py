"""Compare the unchanged public gap data caller and a source policy mutation."""
import argparse
from copy import deepcopy
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
BASELINE = HERE.parent / "gap-range-reference"
REFERENCE_RELEASE = "2c69f8c38cbd57136eb684f5b515ef1490f017b3"
PIN = "af54339d250290490d62d4475a77539b7e739071"
MODEL = Path("crates/marmish-engine/src/model.rs")
SCOPE = "engine-gap-range-v1"
SPEC = "engine-gap-range-package.json"
OLD = "engine-gap-reason-package.json"
CANDIDATE = PORTABLE / SCOPE
JOURNAL = runpy.run_path(str(HERE.parent / "journal-records/verify.py"))
ASSEMBLY = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
TRAIT_OUTPUT = b"public fields, reversed interval, Clone and Eq\n"
ENV = {key: os.environ[key] for key in
       ("HOME", "PATH", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "SDKROOT") if key in os.environ}


def run(*args, **kwargs):
    return subprocess.check_output(args, **kwargs)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def pin(folder, path):
    return {"path": path.relative_to(folder).as_posix(), "sha256": sha(path)}


def checked_pin(folder, descriptor):
    source = folder / descriptor["path"]
    if pin(folder, source) != descriptor:
        raise ValueError(f"input pin differs: {source.name}")
    return source


def clean(tree, commit):
    if run("git", "rev-parse", "HEAD", cwd=tree).decode().strip() != commit or run(
        "git", "status", "--porcelain", "--untracked-files=all", cwd=tree
    ):
        raise ValueError(f"expected clean revision {commit}")


def fixture_reference(original, host):
    fixture = BASELINE / f"original-{host}.json"
    evidence = json.loads(fixture.read_bytes())
    observer = run("git", "show", f"{REFERENCE_RELEASE}:verification/whole/gap-range-reference/observe.py", cwd=ROOT)
    expected = {"original": PIN, "host": host, "modelSha256": sha(original / MODEL),
                "cargoLockSha256": sha(original / "Cargo.lock"),
                "casesSha256": sha(BASELINE / "cases.json"), "probeSha256": sha(BASELINE / "probe.rs"),
                "observerSha256": hashlib.sha256(observer).hexdigest()}
    if any(evidence.get(key) != value for key, value in expected.items()):
        raise ValueError("original reference provenance differs")
    return evidence["results"], sha(fixture)


def mutate_original(original, work):
    destination = work / "original-mutant"
    run("git", "clone", "--quiet", "--local", "--no-checkout", original, destination)
    run("git", "checkout", "--quiet", "--detach", PIN, cwd=destination)
    clean(destination, PIN)
    model = destination / MODEL
    original_bytes = model.read_bytes()
    before = b"#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]\npub struct MissingRange {\n"
    after = before.replace(b"pub struct", b"#[serde(deny_unknown_fields)]\npub struct")
    if original_bytes.count(before) != 1:
        raise ValueError("cannot isolate the source record mutation")
    model.write_bytes(original_bytes.replace(before, after))
    return destination


def observed_mutation(original, target, host, scratch):
    output = run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--lib",
                 "-p", "marmish-engine", "--target", host, "--target-dir", target,
                 "--message-format=json", cwd=original, env=ENV)
    artifacts = list(map(json.loads, output.splitlines()))
    libraries = {}
    for name in ("marmish_engine", "serde_json"):
        row, = [event for event in artifacts if event["reason"] == "compiler-artifact"
                and event["target"]["kind"] == ["lib"] and event["target"]["name"] == name
                and any(Path(path).is_relative_to(target / host) and path.endswith(".rlib")
                        for path in event["filenames"])]
        if name == "marmish_engine" and Path(row["manifest_path"]).resolve() != original / "crates/marmish-engine/Cargo.toml":
            raise ValueError("unexpected native engine artifact")
        libraries[name], = [Path(file) for file in row["filenames"] if file.endswith(".rlib")]
    digests = {name: sha(path) for name, path in libraries.items()}
    extra = sorted({flag for event in artifacts if event["reason"] == "build-script-executed"
                    for flag in event["linked_paths"]})
    flags = ["rustc", "+1.97.1", "--edition=2024", "--target", host, "--cfg", "with_range"]
    for name, path in libraries.items():
        flags.extend(["--extern", f"{name}={path}"])
    for flag in [f"dependency={target / host / 'release/deps'}", f"dependency={target / 'release/deps'}", *extra]:
        flags.extend(["-L", flag])
    exe = scratch / "original-mutant-probe"
    run(*flags, BASELINE / "probe.rs", "-o", exe, cwd=scratch, env=ENV)
    traits = scratch / "original-mutant-traits"
    run(*flags, HERE / "traits.rs", "-o", traits, cwd=scratch, env=ENV)
    if run(traits, cwd=scratch, env=ENV) != TRAIT_OUTPUT:
        raise ValueError("original public traits witness differed")
    observed = run(exe, input=(BASELINE / "cases.json").read_bytes(), cwd=scratch, env=ENV)
    if digests != {name: sha(path) for name, path in libraries.items()}:
        raise ValueError("source mutation library artifacts changed")
    return json.loads(observed), digests


def observed_policy(rows):
    witness, = [row["result"] for row in rows if row["id"] == "unknown-field"]
    if witness == {"encoded": '{"start":0,"end":null,"reason":"Limit"}'}:
        return "ignore"
    if witness.get("diagnostic", "").startswith("unknown field `ignored`"):
        return "reject"
    raise ValueError(f"unrecognized original unknown-field policy: {witness}")


def compose(inputs, expected_policy):
    old = json.loads((PORTABLE / OLD).read_bytes())
    pins = [old["cargoLock"], *(component[key] for component in old["components"] for key in ("graph", "overlay"))]
    for item in pins:
        source = checked_pin(PORTABLE, item)
        if inputs != PORTABLE:
            destination = inputs / item["path"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
    path = inputs / SCOPE
    path.mkdir(exist_ok=True)
    if inputs != PORTABLE:
        graph = json.loads((CANDIDATE / "graph.json").read_bytes())
        value, = graph["modules"][0]["records"]
        if expected_policy == value["readPolicy"]["unknownFields"]:
            shutil.copyfile(CANDIDATE / "graph.json", path / "graph.json")
        else:
            value["readPolicy"]["unknownFields"] = expected_policy
            dump(path / "graph.json", graph)
        shutil.copyfile(CANDIDATE / "rust.json", path / "rust.json")
    descriptor = dict(old, components=[*old["components"], {
        "kind": "public-records", "module": "model", "reexports": ["MissingRange"],
        "graph": pin(inputs, path / "graph.json"), "overlay": pin(inputs, path / "rust.json"),
    }])
    manifest = inputs / SPEC
    dump(manifest, descriptor)
    return manifest


def candidate(work, scratch, args, host, original_results):
    copied = work / "oddlang"
    shutil.copy2(args.compiler, copied)
    prefix, env = JOURNAL["sandbox"](ASSEMBLY, work, (ROOT, args.original, args.compiler.parent, scratch), None)
    for forbidden in (args.original / MODEL, scratch / "original-mutant" / MODEL,
                      BASELINE / "probe.rs", HERE / "traits.rs"):
        denied = subprocess.run([*prefix, "/bin/cat", forbidden], cwd=work, env=env, capture_output=True)
        if (not forbidden.is_file() or denied.returncode != 1 or denied.stdout
                or not any(reason in denied.stderr for reason in (b"No such file", b"Operation not permitted"))):
            raise ValueError(f"source isolation check failed for {forbidden.name}")

    def inside(*cmd, cwd, **extra):
        within = list(prefix)
        if host.endswith("linux-gnu"):
            within[within.index("--chdir") + 1] = str(cwd)
        return run(*within, *cmd, cwd=cwd, env=env, **extra)

    compare = []
    for label, expected in zip(("baseline", "mutation"), original_results, strict=True):
        lane = work / label
        inputs = lane / "inputs"
        inputs.mkdir(parents=True)
        policy = observed_policy(expected)
        manifest = compose(inputs, policy)
        if label == "baseline" and manifest.read_bytes() != (PORTABLE / SPEC).read_bytes():
            raise ValueError("baseline package differs from its input pin")
        product = lane / "product"
        inside(copied, "build", "native-package", "rust", manifest, product, cwd=lane)
        lock = product / "package.lock"
        if label == "baseline" and lock.read_bytes() != (CANDIDATE / "package.lock").read_bytes():
            raise ValueError("candidate package lock changed")
        decoded = json.loads(lock.read_bytes())
        if any(sha(product / path) != digest for path, digest in decoded["generatedFiles"].items()):
            raise ValueError("source tree differs from native package lock")
        inside("cargo", "build", "--locked", "--offline", "--lib", "--release", cwd=product)
        caller = lane / "caller"
        (caller / "src").mkdir(parents=True)
        for source, path in (("caller.toml", "Cargo.toml"), ("caller.lock", "Cargo.lock"), ("probe.rs", "src/main.rs")):
            shutil.copyfile(BASELINE / source, caller / path)
        inside("cargo", "rustc", "--locked", "--offline", "--release", "--", "--cfg", "with_range", cwd=caller)
        observed = json.loads(inside(caller / "target/release/gap-observer", cwd=caller,
                                     input=(BASELINE / "cases.json").read_bytes()))
        if observed != expected:
            pairs = [(a, b) for a, b in zip(observed, expected, strict=True) if a != b]
            raise ValueError(f"projected record differs from the pinned original: {pairs}")
        shutil.copyfile(HERE / "traits.rs", caller / "src/main.rs")
        inside("cargo", "build", "--locked", "--offline", "--release", cwd=caller)
        if inside(caller / "target/release/gap-observer", cwd=caller) != TRAIT_OUTPUT:
            raise ValueError("projected native public traits witness differed")
        compare.append({"unknownFields": policy, "packageLockSha256": sha(lock), "observations": observed})
    return compare


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for flag in ("framework", "compiler_target", "original", "original_target", "receipt"):
        ap.add_argument(flag, type=lambda value: Path(value).resolve())
    ap.add_argument("--framework-revision", required=True)
    args = ap.parse_args()
    clean(args.framework, args.framework_revision)
    clean(args.original, PIN)
    if run("git", "status", "--porcelain", cwd=ROOT):
        raise ValueError("expected a clean companion checkout")
    rustc = run("rustc", "+1.97.1", "-vV", env=ENV).decode().splitlines()
    host, = [line[6:] for line in rustc if line.startswith("host: ")]
    baseline, reference_sha = fixture_reference(args.original, host)
    flags = ("--locked", "--offline", "--bin", "oddlang", "--target-dir", args.compiler_target)
    run("cargo", "+1.96.1", "build", *flags, cwd=args.framework, env=ENV)
    built_compiler = args.compiler_target / "debug/oddlang"
    ids = [row["id"] for row in json.loads((BASELINE / "cases.json").read_bytes())]
    with tempfile.TemporaryDirectory(prefix="range-source-oracle-", dir="/tmp") as tmp:
        scratch = Path(tmp).resolve()
        args.compiler = scratch / "oddlang"
        shutil.copy2(built_compiler, args.compiler)
        compiler_sha = sha(args.compiler)
        source = mutate_original(args.original, scratch)
        mutated_source_sha = sha(source / MODEL)
        mutant, library_hashes = observed_mutation(source, args.original_target, host, scratch)
        if sha(source / MODEL) != mutated_source_sha or run(
            "git", "status", "--porcelain", "--untracked-files=all", cwd=source
        ) != b" M crates/marmish-engine/src/model.rs\n":
            raise ValueError("the controlled source mutation changed during observation")
        for rows in (baseline, mutant):
            if len(set(ids)) != 35 or [row["id"] for row in rows] != ids:
                raise ValueError("missing or extra observed rows")
        changed = [a["id"] for a, b in zip(baseline, mutant, strict=True) if a != b]
        if changed != ["unknown-field"] or (observed_policy(baseline), observed_policy(mutant)) != ("ignore", "reject"):
            raise ValueError("the original source mutation changed unexpected observations")
        with tempfile.TemporaryDirectory(prefix="sealed-range-") as tmp:
            generated = candidate(Path(tmp).resolve(), scratch, args, host, [baseline, mutant])
    dump(args.receipt, {
        "format": "marmish-public-gap-range-v1", "original": PIN,
        "framework": args.framework_revision, "compilerSha256": compiler_sha,
        "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "rustc": rustc, "originalReferenceSha256": reference_sha, "sourceDenials": 4,
        "nativeTraitsProbeSha256": sha(HERE / "traits.rs"),
        "sourceModelSha256": sha(args.original / MODEL), "unchangedProbeSha256": sha(BASELINE / "probe.rs"),
        "casesSha256": sha(BASELINE / "cases.json"), "mutationSourceModelSha256": mutated_source_sha,
        "mutationLibrarySha256": library_hashes, "changedRows": changed, "baseline": generated[0],
        "mutation": generated[1],
    })


if __name__ == "__main__":
    main()
