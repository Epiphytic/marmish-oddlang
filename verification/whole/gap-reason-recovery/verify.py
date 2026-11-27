"""Recover a pinned native reason and replay its baseline and source mutation."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BUNDLE = PORTABLE / "engine-gap-reason-recovered-v1"
REFERENCE = HERE.parent / "gap-range-reference"
MANUAL = PORTABLE / "engine-gap-reason-v1"
MANIFEST = PORTABLE / "engine-gap-reason-package.json"
SOURCE = Path("crates/marmish-engine/src/model.rs")
PROBE = HERE / "probe.rs"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
RUSTC = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
CARGO = ("cargo", "+1.97.1")
ARTIFACTS = ("graph.json", "rust.json", "recovery.lock")


def read(path):
    return json.loads(path.read_bytes())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*argv, cwd=None, env=None, stdin=None):
    result = subprocess.run(argv, cwd=cwd, env=env, input=stdin, capture_output=True, timeout=1200)
    if result.returncode:
        raise RuntimeError(f"{argv!r}: {result.stderr.decode(errors='replace')}")
    return result.stdout


def clean_pin(checkout, revision=None):
    actual = run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip()
    if (revision is not None and revision != actual) or run(
        "git", "status", "--porcelain", "--untracked-files=all", cwd=checkout,
    ):
        raise ValueError(f"unexpected or dirty pin: {checkout}")
    return actual


def original_mutation(original, work):
    mutated = work / "mutated-application"
    mutated.mkdir()
    archive = work / "original.tar"
    with archive.open("wb") as output:
        subprocess.run(["git", "-C", original, "archive", ORIGINAL], stdout=output, check=True)
    run("tar", "-xf", archive, "-C", mutated)
    archive.unlink()
    source = mutated / SOURCE
    raw = source.read_bytes()
    before = b"pub enum MissingReason {\n    Limit,\n    StorageFailure,\n"
    after = b'pub enum MissingReason {\n    #[serde(rename = "Storage-Failure")]\n    StorageFailure,\n    Limit,\n'
    if raw.count(before) != 1:
        raise ValueError("pinned reason declaration changed")
    source.write_bytes(raw.replace(before, after, 1))
    selection = read(BUNDLE / "selection.json")
    selection["sourceSha256"] = sha(source)
    pin = work / "changed-selection.json"
    write(pin, selection)
    return mutated, pin


def reference_products(root, target, host, env):
    cargo = [*CARGO, "build", "--lib", "--release", "--locked", "--offline", "--target", host,
             "--target-dir", target, "--message-format=json", "-p", "marmish-engine"]
    records = [json.loads(line) for line in run(*cargo, cwd=root, env=env).splitlines()]
    libraries = {}
    for name in ("marmish_engine", "serde_json"):
        items = [(row, Path(value)) for row in records
                 if row["reason"] == "compiler-artifact" and row["target"]["name"] == name
                 and row["target"]["kind"] == ["lib"] for value in row["filenames"]
                 if value.endswith(".rlib") and Path(value).is_relative_to(target / host)]
        (row, library), = items
        if name == "marmish_engine":
            manifest = root / "crates/marmish-engine/Cargo.toml"
            if Path(row["manifest_path"]).resolve() != manifest:
                raise ValueError("native type did not come from the selected package")
        libraries[name] = library
    links = {link for row in records if row["reason"] == "build-script-executed"
             for link in row["linked_paths"]}
    return libraries, sorted(links)


def copy_rejection(flags, work, env):
    source = work / "copy.rs"
    source.write_text("fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }")
    result = subprocess.run([*flags, "--error-format=json", "--emit=metadata", source, "-o", work / "reject.rmeta"],
                            capture_output=True, cwd=work, env=env)
    codes = [row["code"]["code"] for row in map(json.loads, result.stderr.splitlines())
             if row.get("level") == "error" and row.get("code")]
    if result.returncode == 0 or codes != ["E0277"]:
        raise ValueError(f"wrong Copy diagnostic: {result.stderr.decode()}")
    return codes


def observe_reference(root, work, target, host, env, cases, lane):
    work.mkdir()
    libraries, links = reference_products(root, target, host, env)
    hashes = {name: sha(path) for name, path in libraries.items()}
    flags = ["rustc", "+1.97.1", "--edition=2024", "--target", host]
    for name, library in sorted(libraries.items()):
        flags.extend(("--extern", f"{name}={library}"))
    for path in [f"dependency={target / host / 'release/deps'}", f"dependency={target / 'release/deps'}", *links]:
        flags.extend(("-L", path))
    executable = work / "probe"
    run(*flags, PROBE, "-o", executable, cwd=work, env=env)
    rows = json.loads(run(executable, lane, cwd=work, env=env, stdin=json.dumps(cases).encode()))
    codes = copy_rejection(flags, work, env)
    if hashes != {name: sha(path) for name, path in libraries.items()}:
        raise ValueError("reference library changed during qualification")
    return {"output": rows, "copyControl": codes}, hashes


def observe_candidate(product, work, host, env, cases, lane):
    caller = work / "caller"
    sources = caller / "src"
    sources.mkdir(parents=True)
    if product != caller.parent / "product":
        raise ValueError("candidate path differs from the pinned caller dependency")
    shutil.copyfile(REFERENCE / "caller.toml", caller / "Cargo.toml")
    shutil.copyfile(REFERENCE / "caller.lock", caller / "Cargo.lock")
    source = sources / "main.rs"
    shutil.copyfile(PROBE, source)
    flags = ("--release", "--offline", "--locked", "--target", host)
    got = json.loads(run(*CARGO, "run", *flags, "--quiet", "--", lane,
                         cwd=caller, env=env, stdin=json.dumps(cases).encode()))
    source.write_text("fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }")
    result = subprocess.run([*CARGO, "check", *flags, "--message-format=json"],
                            cwd=caller, env=env, capture_output=True)
    # Cargo emits rustc diagnostics as structured compiler-message rows.
    codes = [row["message"]["code"]["code"] for row in map(json.loads, result.stdout.splitlines())
             if row.get("reason") == "compiler-message" and row["message"]["level"] == "error"
             and row["message"].get("code")]
    if result.returncode == 0 or codes != ["E0277"]:
        raise ValueError(f"wrong generated Copy diagnostic: {result.stderr.decode()}")
    return {"output": got, "copyControl": codes}


def verify(args, work):
    app_revision = clean_pin(ROOT)
    clean_pin(args.original, ORIGINAL)
    clean_pin(args.framework, args.framework_revision)
    env = {key: os.environ[key] for key in ("HOME", "PATH", "CARGO_HOME", "TMPDIR", "RUSTUP_HOME", "SDKROOT")
           if key in os.environ}
    version = run("rustc", "+1.97.1", "-vV", env=env).decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if f"commit-hash: {RUSTC}" not in version or host not in {"aarch64-apple-darwin", "aarch64-unknown-linux-gnu"}:
        raise ValueError("unsupported Rust compiler or native qualification host")
    run(*CARGO, "build", "--locked", "--offline", "--bin", "oddlang", "--target-dir", args.compiler_target,
        cwd=args.framework, env=env)
    compiler = work / "oddlang"
    shutil.copy2(args.compiler_target / "debug/oddlang", compiler)
    manifest = HERE.parent / "canonical-public-data/Cargo.toml"
    run(*CARGO, "build", "--locked", "--offline", "--manifest-path", manifest,
        "--bin", "gap-reason-contract", "--target-dir", work / "tools", env=env)
    recovery = work / "tools/debug/gap-reason-contract"
    original_contract = json.loads(run(recovery, compiler, args.original / SOURCE,
                                       BUNDLE / "selection.json", work / "baseline", env=env))
    locked = original_contract["recovery"]
    baseline = work / "baseline/recovered"
    for name in ARTIFACTS:
        if (baseline / name).read_bytes() != (BUNDLE / name).read_bytes():
            raise ValueError(f"pinned source recovery drift: {name}")
    for name in ARTIFACTS[:2]:
        if read(baseline / name) != read(MANUAL / name):
            raise ValueError(f"recovered contract differs from independent baseline: {name}")

    mutant, selected = original_mutation(args.original, work)
    mutation_contract = json.loads(run(recovery, compiler, mutant / SOURCE, selected, work / "mutation", env=env))
    mutant_lock = mutation_contract["recovery"]
    changed = work / "mutation/recovered"
    graph, overlay = read(baseline / "graph.json"), read(baseline / "rust.json")
    choice, = graph["stringEnums"]
    variants = choice["variants"]
    variants[1]["wire"] = "Storage-Failure"
    variants[:2] = variants[:2][::-1]
    overlay["variantOrder"][choice["id"]][:2] = overlay["variantOrder"][choice["id"]][:2][::-1]
    if graph != read(changed / "graph.json") or overlay != read(changed / "rust.json"):
        raise ValueError("source mutation changed more than the expected spelling and declaration order")

    reference_path = REFERENCE / f"original-{host}.json"
    reference = read(reference_path)
    catalog = [case for case in read(REFERENCE / "cases.json") if case["type"] == "reason"]
    identifiers = {case["id"] for case in catalog}
    expected = [row for row in reference["results"] if row["id"] in identifiers]
    if reference["modelSha256"] != sha(args.original / SOURCE) or reference["casesSha256"] != sha(REFERENCE / "cases.json") or len(expected) != 17:
        raise ValueError("unexpected baseline reason evidence")
    mutation_cases = [*catalog, {"id": "new-storage-wire-name", "type": "reason", "wire": '"Storage-Failure"'}]
    witnesses = {}
    if args.reference_target == args.mutation_target:
        raise ValueError("use separate native target directories for original and mutated sources")
    references = {}
    for lane, native, target, cases in (
        ("baseline", args.original, args.reference_target, catalog),
        ("mutation", mutant, args.mutation_target, mutation_cases),
    ):
        target.mkdir(parents=True, exist_ok=True)
        with (target / ".oddlang-missing-reason-reference.lock").open("a") as gate:
            fcntl.flock(gate, fcntl.LOCK_EX)
            references[lane] = observe_reference(native, work / (lane + "-original"), target, host, env, cases, lane)
    for lane, contract, cases in (("baseline", original_contract, catalog), ("mutation", mutation_contract, mutation_cases)):
        build = work / lane
        product, emitted = build / "product", contract["candidate"]
        got = observe_candidate(product, build, host, env, cases, lane)
        original, libraries = references[lane]
        if got != original:
            raise ValueError(f"candidate {lane} differs from original: {got!r} != {original!r}")
        rows = got["output"]["observations"]
        if lane == "baseline":
            if rows != expected:
                raise ValueError("recovered candidate differs from the independent baseline witnesses")
            manual = read(MANUAL / "package.lock")
            if emitted["generatedFiles"] != manual["generatedFiles"] or emitted["projection"] != manual["components"][0]["projection"]:
                raise ValueError("source recovery changed the independently authored package projection")
        else:
            old_ids = {case["id"] for case in catalog if case["wire"] == '"StorageFailure"'}
            by_id = {row["id"]: row["result"] for row in rows}
            if not old_ids or any("diagnostic" not in by_id[identity] for identity in old_ids) or (
                by_id["new-storage-wire-name"] != {"encoded": '"Storage-Failure"'}
            ):
                raise ValueError("source mutation failed to change native wire admission")
        witnesses[lane] = {"candidate": emitted, "originalLibrarySha256": libraries, **got}
    clean_pin(args.original, ORIGINAL)
    return {"format": "marmish-gap-reason-recovery-v1", "host": host, "original": ORIGINAL,
            "framework": args.framework_revision, "companion": app_revision, "rustc": RUSTC,
            "compilerSha256": sha(compiler), "sourceSha256": sha(args.original / SOURCE),
            "probeSha256": sha(PROBE), "referenceSha256": sha(reference_path),
            "baselineRecovery": locked, "mutationRecovery": mutant_lock, "lanes": witnesses}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "output"):
        cli.add_argument(name, type=lambda value: Path(value).resolve())
    for name in ("compiler-target", "reference-target", "mutation-target"):
        cli.add_argument("--" + name, type=lambda value: Path(value).resolve(), required=True)
    cli.add_argument("--framework-revision", required=True)
    args = cli.parse_args()
    with tempfile.TemporaryDirectory(prefix="oddlang-gap-reason-source-") as tmp:
        receipt = verify(args, Path(tmp).resolve())
    write(args.output, receipt)
