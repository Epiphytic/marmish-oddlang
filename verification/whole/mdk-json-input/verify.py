"""Qualify only supplied-byte JSON parsing and a private unchanged MDK case."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
DOMAIN = "mdk-json-input-v1"
MANIFEST = "mdk-json-input-package.json"
PIN = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = "crates/marmish-mdk/src/lib.rs"
TESTS = "crates/marmish-mdk/src/tests.rs"
CASE = "synthetic_json_and_ndjson_are_bounded_and_structural_only"
PROBE = "bounded_metadata_observation::discover_bounds"
LEAF = Path("src/group_2/__projection_2.rs")
JOURNAL = runpy.run_path(str(HERE.parent / "journal-records/verify.py"))
ASSEMBLY = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
ERROR = "urn:marmish:mdk:adapter-error:v1:variant:"


def run(*args, cwd=None, env=None):
    return subprocess.run(args, cwd=cwd, env=env, check=True, stdout=subprocess.PIPE).stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def pin(base, path):
    return {"path": path.relative_to(base).as_posix(), "sha256": sha(path.read_bytes())}


def case_bytes(original):
    data = run("git", "-C", original, "show", f"{PIN}:{TESTS}")
    head = f"#[test]\nfn {CASE}()".encode()
    if data.count(head) != 1:
        raise ValueError("pinned MDK test is ambiguous")
    start = data.index(head)
    test = data[start:data.index(b"\n#[test]", start + len(head))]
    if sha(test) != "04916e33e73f6e8856688823f8af5af965677a7403e195477de9f4a19d1a6ad7":
        raise ValueError("unchanged MDK test digest differs")
    return test


def cargo_test(crate, target, name, *, original):
    args = ["cargo", "+1.97.1", "test", "--release", "--locked", "--offline",
            "--lib", "--target-dir", target]
    if original:
        args.extend(["-p", "marmish-mdk"])
    stdout = run(*args, "--", "--exact", name, "--nocapture", cwd=crate, env=JOURNAL["cargo_env"]())
    if (stdout.splitlines().count(f"test {name} ... ok".encode()) != 1
            or stdout.count(b"test result: ok. 1 passed; 0 failed; 0 ignored;") != 1):
        raise ValueError(f"expected exact execution of {name}: {stdout!r}")
    return stdout


def observe(crate, target, name, *, original):
    stdout = cargo_test(crate, target, name, original=original)
    row, = [line.removeprefix(b"BOUNDED_METADATA=") for line in stdout.splitlines()
            if line.startswith(b"BOUNDED_METADATA=")]
    return json.loads(row)


def observe_original(original, target, scratch):
    clone = scratch / "pinned"
    run("git", "clone", "--quiet", "--local", "--no-checkout", original, clone)
    run("git", "-C", clone, "checkout", "--quiet", "--detach", PIN)
    cargo_test(clone, target, "tests::" + CASE, original=True)
    source = clone / SOURCE
    old = source.read_bytes()
    probe = (HERE / "observe.rs").read_bytes()
    source.write_bytes(old + b"\n" + probe)
    baseline = observe(clone, target, PROBE, original=True)
    edits = [(b"= Limits::NDJSON_BYTES;", b"= Limits::NDJSON_BYTES / 2;"),
             (b"records.len() >= 200", b"records.len() >= 181")]
    changed = old
    for old_text, new_text in edits:
        if changed.count(old_text) != 1:
            raise ValueError("source mutation does not identify one policy site")
        changed = changed.replace(old_text, new_text)
    source.write_bytes(changed + b"\n" + probe)
    mutant = observe(clone, target, PROBE, original=True)
    if (mutant["acceptedValueBytes"] * 2 != baseline["acceptedValueBytes"]
            or mutant["acceptedRecords"] != 181 or baseline["acceptedRecords"] == 181
            or baseline["cases"] != mutant["cases"]):
        raise ValueError("original source mutation did not discover both changed bounds")
    return baseline, mutant, sha(old), sha(changed)


def recovery(compiler, observed, work, output):
    work.mkdir()
    obs = work / "observation.json"
    write(obs, observed)
    selection = work / "selection.json"
    write(selection, {
        "format": "oddlang-bounded-json-selection-v1", "id": "urn:marmish:mdk:synthetic-metadata:v1",
        "observationSha256": sha(obs.read_bytes()),
        "capacityError": ERROR + "OutputLimit", "invalidError": ERROR + "InvalidResponse",
        "binding": {
            "format": "oddlang-bounded-json-rust-v1", "single": "parse_json", "sequence": "parse_ndjson",
            "limit": "OUTPUT_LIMIT", "errorType": "AdapterError",
            "variants": {ERROR + "OutputLimit": "OutputLimit", ERROR + "InvalidResponse": "InvalidResponse"},
        },
    })
    run(compiler, "recover", "bounded-json-input", obs, selection, output)


def compose(portable, out, recovered):
    package = json.loads((portable / "mdk-public-data-package.json").read_bytes())
    for part in package["components"]:
        for role in ("graph", "overlay"):
            descriptor = part[role]
            source = portable / descriptor["path"]
            if out != portable:
                destination = out / descriptor["path"]
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
            if pin(out, out / descriptor["path"]) != descriptor:
                raise ValueError("prior MDK public component changed")
    inputs = out / DOMAIN
    inputs.mkdir(exist_ok=True)
    names = ["graph.json", "rust.json"]
    for name in names:
        if (recovered / name).resolve() != (inputs / name).resolve():
            shutil.copy2(recovered / name, inputs)
    lock = portable / DOMAIN / "Cargo.lock"
    if out != portable:
        shutil.copy2(lock, inputs)
    package["cargoLock"] = pin(out, inputs / lock.name)
    package["dependencies"] = {"serde_json": {"version": "1.0.151"}}
    package["components"].append({
        "kind": "bounded-json-input", "module": "metadata", "reexports": [],
        "graph": pin(out, inputs / names[0]), "overlay": pin(out, inputs / names[1]),
    })
    path = out / MANIFEST
    write(path, package)
    return path


def sealed(work, scratch, args, recovered, mutated):
    compiler = work / "oddlang"
    shutil.copy2(args.compiler, compiler)
    prefix, env = JOURNAL["sandbox"](ASSEMBLY, work, (ROOT, args.original, args.compiler.parent, scratch), None)
    probes = [args.original / SOURCE, args.original / TESTS, HERE / "observe.rs", scratch / "pinned" / SOURCE]
    for probe in probes:
        call = subprocess.run([*prefix, "/bin/cat", probe], env=env, cwd=work, capture_output=True)
        if (not probe.is_file() or call.returncode != 1 or call.stdout
                or not any(reason in call.stderr for reason in (b"Operation not permitted", b"No such file"))):
            raise ValueError(f"source isolation failed for {probe.name}")
    result = []
    for label, graph in [("baseline", recovered), ("changed", mutated)]:
        inputs = work / label
        inputs.mkdir()
        manifest = compose(PORTABLE, inputs, graph)
        if label == "baseline" and manifest.read_bytes() != (PORTABLE / MANIFEST).read_bytes():
            raise ValueError("staged baseline package differs from the published descriptor")
        if any(path.suffix not in {".json", ".lock"} for path in inputs.rglob("*") if path.is_file()):
            raise ValueError("unexpected source input")
        product = work / f"product-{label}"
        run(*prefix, compiler, "build", "native-package", "rust", manifest, product, cwd=work, env=env)
        run(*prefix, "cargo", "build", "--locked", "--offline", "--release",
            "--manifest-path", product / "Cargo.toml", cwd=work, env=env)
        lock = sha((product / "package.lock").read_bytes())
        with (product / LEAF).open("ab") as code:
            code.write(b"\n" + (HERE / "observe.rs").read_bytes())
            if label == "baseline":
                code.write(b"\n#[cfg(test)] mod tests {\nuse super::{parse_json, parse_ndjson, OUTPUT_LIMIT};\n"
                           b"use crate::AdapterError;\n" + case_bytes(args.original) + b"\n}\n")
        target = product / "target"
        if label == "baseline":
            cargo_test(product, target, f"metadata::__projection_2::tests::{CASE}", original=False)
        observed = observe(product, target, "metadata::__projection_2::" + PROBE, original=False)
        result.append({"packageLockSha256": lock, "observations": observed,
                       "recovery": json.loads((graph / "recovery.lock").read_bytes())})
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for key in ("framework", "compiler_target", "original", "original_target", "receipt"):
        ap.add_argument(key, type=lambda value: Path(value).resolve())
    ap.add_argument("--framework-revision", required=True)
    ap.add_argument("--prepare", action="store_true")
    args = ap.parse_args()
    if run("git", "-C", args.original, "rev-parse", "HEAD").decode().strip() != PIN or run(
        "git", "-C", args.original, "status", "--porcelain", "--untracked-files=all"
    ):
        raise ValueError("expected a clean pinned original")
    if run("git", "rev-parse", "HEAD", cwd=args.framework).decode().strip() != args.framework_revision:
        raise ValueError("wrong framework revision")
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=args.framework):
        raise ValueError("expected clean framework checkout")
    if not args.prepare and run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT):
        raise ValueError("expected clean companion checkout")
    run("cargo", "+1.97.1", "build", "--offline", "--locked", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
    args.compiler = args.compiler_target / "debug/oddlang"
    test = case_bytes(args.original)
    with tempfile.TemporaryDirectory(prefix="odd-mdk-oracle-") as s:
        scratch = Path(s).resolve()
        baseline, changed, original_sha, changed_sha = observe_original(args.original, args.original_target, scratch)
        recovered, mutated = scratch / "recovered", scratch / "mutated"
        recovery(args.compiler, baseline, scratch / "capture", recovered)
        recovery(args.compiler, changed, scratch / "capture-mutant", mutated)
        if args.prepare:
            compose(PORTABLE, PORTABLE, recovered)
            return
        for name in ("graph.json", "rust.json"):
            if (recovered / name).read_bytes() != (PORTABLE / DOMAIN / name).read_bytes():
                raise ValueError("recovered metadata contract differs from its pin")
        with tempfile.TemporaryDirectory(prefix="odd-mdk-sealed-") as w:
            results = sealed(Path(w).resolve(), scratch, args, recovered, mutated)
        if [result["observations"] for result in results] != [baseline, changed]:
            raise ValueError("native recovery and projected controls disagree")
        write(args.receipt, {
            "format": "marmish-private-mdk-json-input-v1", "original": PIN,
            "framework": args.framework_revision, "compilerSha256": sha(args.compiler.read_bytes()),
            "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
            "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
            "case": f"tests::{CASE}", "caseSha256": sha(test), "caseCount": 1,
            "probeSha256": sha((HERE / "observe.rs").read_bytes()),
            "originalSourceSha256": original_sha, "changedSourceSha256": changed_sha,
            "baseline": results[0], "mutation": results[1], "deniedSourcePaths": 4,
        })


if __name__ == "__main__":
    main()
