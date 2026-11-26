"""Qualify all four public data additions from one sealed native workspace build."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

from compose import BASE, ENGINE, NATIVE, OUTPUT, PORTABLE, compose, digest, read

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WHOLE = HERE.parent
FRAMEWORK = "900cfa517d0136fa80c2b5e92ccf9b1a3b2a71ab"
DARWIN, LINUX = "aarch64-apple-darwin", "aarch64-unknown-linux-gnu"
MANIFEST = PORTABLE / f"{OUTPUT}.json"
LOCK = PORTABLE / OUTPUT / "workspace.lock"
ADDED = {
    ("engine", "Stream"), ("engine", "SubmitDisposition"), ("engine", "MissingReason"),
    ("native", "mdk_adapter::RuntimeStatus"),
}


def lane(directory, script="verify.py"):
    path = WHOLE / directory
    prior_path = sys.path.copy()
    modules = {key: sys.modules.pop(key) for key in ("compose", "verify") if key in sys.modules}
    sys.path.insert(0, str(path))
    try:
        return runpy.run_path(str(path / script))
    finally:
        sys.path[:] = prior_path
        for key in ("compose", "verify"):
            sys.modules.pop(key, None)
        sys.modules.update(modules)


SIX = lane("unified-mdk")
JOURNAL = SIX["JOURNAL"]
PREVIOUS = lane("canonical-linux-imports")
AUDIT = PREVIOUS["audit"]
ENGINE_DATA = lane("engine-fieldless-choices")
STATUS_DATA = lane("runtime-status-data")
REASON = lane("gap-reason-recovery")
run, data = SIX["run"], SIX["data"]


def changed_files(before, after):
    first, last = (data(package / "package.lock")["generatedFiles"] for package in (before, after))
    if first.keys() - last.keys():
        raise ValueError("qualified files disappeared")
    return {name for name in first if first[name] != last[name]}, set(last) - set(first)


def snapshots(baseline, product, revision):
    old = PREVIOUS["CANONICAL"]
    current = AUDIT.Candidate(OUTPUT, revision, FRAMEWORK, digest(LOCK))
    AUDIT.verify_product(baseline, ROOT, old)
    AUDIT.verify_product(product, ROOT, current)
    before, after = (data(p / "workspace.lock") for p in (baseline, product))
    if any(after[k] != before[k] for k in ("format", "workspaceCargoManifestSha256")) or (
        after["members"].keys() != before["members"].keys()
    ):
        raise ValueError("root workspace Cargo structure changed")
    if (product / "Cargo.lock").read_bytes() != (PORTABLE / OUTPUT / "Cargo.lock").read_bytes():
        raise ValueError("generated root Cargo lock changed")
    for name, row in after["members"].items():
        inherited = before["members"][name]
        allowed = {
            ENGINE: {"inputSha256", "standalonePackageLockSha256", "workspaceCargoManifestSha256"},
            NATIVE: {"inputSha256", "standalonePackageLockSha256"},
        }.get(name, set())
        if {key for key in row if row[key] != inherited[key]} != allowed:
            raise ValueError(f"unexpected workspace member delta: {name}")

    engine_before, engine = baseline / ENGINE, product / ENGINE
    prior_engine, recovered = (data(p / "package.lock") for p in (engine_before, engine))
    choice_lock_path = PORTABLE / "engine-fieldless-choices-v1/package.lock"
    choices = data(choice_lock_path)
    choice_workspace = data(PORTABLE / "workspace-v7-canonical-engine-choices/workspace.lock")
    if choice_workspace["members"][ENGINE]["standalonePackageLockSha256"] != digest(choice_lock_path):
        raise ValueError("canonical fieldless engine package differs from standalone qualification")
    reason = data(PORTABLE / "engine-gap-reason-v1/package.lock")["components"][0]
    if recovered["components"][:-1] != choices["components"] or recovered["components"][:-2] != prior_engine["components"]:
        raise ValueError("inherited engine projection changed")
    wire = recovered["components"][-1]
    if wire["projection"] != reason["projection"] or wire["sourceSha256"] != reason["sourceSha256"]:
        raise ValueError("combined missing reason projection differs from verified output")
    for field, item in (("graphSha256", "graph.json"), ("overlaySha256", "rust.json")):
        if wire[field] != digest(PORTABLE / "engine-gap-reason-recovered-v1" / item):
            raise ValueError("combined MissingReason did not use source-recovered bytes")
    changed, added = changed_files(engine_before, engine)
    if changed != {"src/lib.rs", "src/group_1/mod.rs"} or added != {
        "src/group_1/__projection_3.rs", "src/group_1/__projection_4.rs",
    }:
        raise ValueError("unexpected generated engine source changes")
    for path, tail in (
        ("src/lib.rs", "pub use model::{Stream, SubmitDisposition};\npub use model::{MissingReason};\n"),
        ("src/group_1/mod.rs", "mod __projection_3;\npub use __projection_3::{Stream, SubmitDisposition};\n"
         "mod __projection_4;\npub use __projection_4::{MissingReason};\n"),
    ):
        if (engine / path).read_bytes() != (engine_before / path).read_bytes() + tail.encode():
            raise ValueError(f"unexpected engine export structure: {path}")

    native_before, native = baseline / NATIVE, product / NATIVE
    original_native, modified_native = (data(p / "package.lock") for p in (native_before, native))
    native_receipt = data(PORTABLE / "workspace-v7-canonical-runtime-status/workspace.lock")["members"][NATIVE]
    if after["members"][NATIVE] != native_receipt:
        raise ValueError("combined native member differs from separately qualified RuntimeStatus")
    index, = [i for i, component in enumerate(original_native["components"]) if component["module"] == "record_views"]
    expected = original_native["components"].copy()
    expected[index] = modified_native["components"][index]
    native_changed, native_added = changed_files(native_before, native)
    (leaf,) = [path for path in original_native["generatedFiles"] if path.endswith(f"/__projection_{index}.rs")]
    if modified_native["components"] != expected or native_changed != {leaf} or native_added:
        raise ValueError("native status should change only its existing generated record leaf")
    return {
        "canonicalWorkspaceLockSha256": old.lock_sha256,
        "candidateWorkspaceLockSha256": current.lock_sha256,
        "unchangedMemberLocks": sorted(set(after["members"]) - {ENGINE, NATIVE}),
        "memberLocks": after["members"],
        "rootCargoLockSha256": after["cargoLockSha256"],
        "rootCargoManifestSha256": after["workspaceCargoManifestSha256"],
        "engineChangedFiles": sorted(changed),
        "engineAddedFiles": sorted(added),
        "nativeChangedFiles": sorted(native_changed),
    }


def previous_imports(host):
    if host == DARWIN:
        receipt = data(PREVIOUS["DARWIN"])
        if receipt["workspaceLockSha256"] != PREVIOUS["CANONICAL"].lock_sha256:
            raise ValueError("wrong canonical Darwin evidence")
        return receipt["darwinPublicImports"]["libraries"]
    receipt = data(WHOLE / "canonical-linux-imports" / f"{host}.json")
    if receipt["candidate"] != PREVIOUS["CANONICAL"]._asdict():
        raise ValueError("wrong canonical Linux evidence")
    return receipt["generatedBuild"]["libraries"]


def import_witness(host, roles, original, scratch):
    prior = previous_imports(host)
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as jobs:
        futures = {name: jobs.submit(AUDIT.imports, name, roles[f"marmish-{name}"], original, target=host)
                   for name in AUDIT.COUNTS}
        libraries = {name: future.result() for name, future in futures.items()}
    added = set()
    baseline_count = 0
    count = 0
    for name, found in libraries.items():
        previous = prior[name]
        for key in ("inventorySha256", "inventoryLockSha256", "inventoryEvidenceSha256"):
            if found[key] != previous[key]:
                raise ValueError("public import inventory changed")
        for old, new in zip(previous["imports"], found["imports"], strict=True):
            if (old["spelling"], old["originalInventoryKind"]) != (new["spelling"], new["originalInventoryKind"]):
                raise ValueError("public import identity changed")
            baseline_count += old["compiles"]
            count += new["compiles"]
            if new["compiles"] and not old["compiles"]:
                if [error["code"] for error in old["diagnostics"]] != ["E0432"]:
                    raise ValueError(f"unexpected prior error for new import: {new['spelling']}")
                added.add((name, new["spelling"]))
            elif old["compiles"] != new["compiles"] or [
                (error["code"], error["message"]) for error in old["diagnostics"]
            ] != [(error["code"], error["message"]) for error in new["diagnostics"]]:
                raise ValueError(f"inherited import outcome changed: {name}::{new['spelling']}")
    attempted = sum(len(library["imports"]) for library in libraries.values())
    if attempted != 140 or baseline_count != 77 or count != 81 or added != ADDED:
        raise ValueError(f"unexpected public import totals or gains: {attempted}, {baseline_count}, {count}, {added}")
    controls = {}
    for key, spelling, code in (("absent", "NoSuchAuditSentinel", "E0432"), ("private", "config", "E0603")):
        actual = AUDIT.compiler_probe(scratch, roles["marmish-core"], f"pub use observed::{spelling};", target=host)
        if actual["compiles"] or [error["code"] for error in actual["diagnostics"]] != [code]:
            raise ValueError(f"unexpected native import control: {key}")
        controls[key] = actual
    return {"attempted": attempted, "baseline": baseline_count, "candidate": count,
            "added": sorted(added), "controls": controls, "libraries": libraries}


def dependency(rows, name, target_release):
    path, = {
        Path(filename)
        for row in rows if row["reason"] == "compiler-artifact" and row["target"]["name"] == name
        and row["target"]["kind"] == ["lib"] and row["profile"]["opt_level"] == "3"
        for filename in row["filenames"]
        if filename.endswith(".rlib") and Path(filename).is_relative_to(target_release)
    }
    return path


def domains(host, raw, target, roles, links, work):
    engine, native = roles[ENGINE][1], roles[NATIVE][1]
    rows = [json.loads(line) for line in raw.splitlines()]
    release = target / host / "release"
    serde = dependency(rows, "serde", release)
    serde_json = dependency(rows, "serde_json", release)
    engine_data = ENGINE_DATA["observe"](engine, serde, target / "release/deps", links, host,
                                        work / "engine-fieldless", False, JOURNAL["cargo_env"]())
    fieldless_path = WHOLE / "engine-fieldless-choices" / f"{host}.json"
    fieldless = data(fieldless_path)["scenarios"]["baseline"]
    if engine_data != {name: fieldless[name] for name in ("values", "traits")}:
        raise ValueError("combined engine choice caller differs from pinned standalone witness")

    runtime, native_hash, _ = STATUS_DATA["caller"](work / "native-status", raw, native)
    runtime_path = WHOLE / "runtime-status-data" / f"{host}.json"
    runtime_receipt = data(runtime_path)
    if runtime != runtime_receipt["observations"]:
        raise ValueError("combined RuntimeStatus differs from pinned external caller")

    reason_root = work / "missing-reason"
    reason_root.mkdir()
    argv = ["rustc", "+1.97.1", "--edition=2024", "--target", host,
            "--extern", f"marmish_engine={engine}", "--extern", f"serde_json={serde_json}",
            "-L", f"dependency={release / 'deps'}", "-L", f"dependency={target / 'release/deps'}"]
    argv.extend(arg for path in links for arg in ("-L", path))
    binary = reason_root / "probe"
    run(*argv, REASON["PROBE"], "-o", binary)
    catalog = [row for row in data(REASON["REFERENCE"] / "cases.json") if row["type"] == "reason"]
    observed = json.loads(REASON["run"](binary, "baseline", cwd=reason_root, stdin=json.dumps(catalog).encode()))
    copy = REASON["copy_rejection"](argv, reason_root, JOURNAL["cargo_env"]())
    reason_path = WHOLE / "gap-reason-recovery" / f"{host}.json"
    reason_receipt = data(reason_path)["lanes"]["baseline"]
    if len(catalog) != 17 or observed != reason_receipt["output"] or copy != reason_receipt["copyControl"]:
        raise ValueError("combined MissingReason differs from pinned native wire and type witnesses")
    return {
        "fieldlessChoices": engine_data, "runtimeStatus": runtime,
        "missingReason": {"output": observed, "copyControl": copy},
        "candidateEngineLibrarySha256": digest(engine), "candidateNativeLibrarySha256": native_hash,
        "originalWitnessReceiptSha256": {
            "fieldlessChoices": digest(fieldless_path), "runtimeStatus": digest(runtime_path),
            "missingReason": digest(reason_path),
        },
    }


def qualify(args, scratch):
    for path, pin in ((args.framework, FRAMEWORK), (ROOT, args.input_revision), (args.original, AUDIT.ORIGINAL)):
        ENGINE_DATA["checkout"](path, pin)
    if any(args.work.is_relative_to(path) for path in (ROOT, args.original, args.framework)):
        raise ValueError("use a separate assembly area")
    compose(check=True)
    PREVIOUS["frozen_inputs"](ROOT)
    source = PREVIOUS["source_pin"](args.original)
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host not in {DARWIN, LINUX} or f"commit-hash: {AUDIT.COMPILER}" not in version:
        raise ValueError("expected native pinned Rust 1.97.1")
    if args.prebuilt is None:
        run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
            "--target-dir", args.compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
        executable = args.compiler_target / "debug/oddlang"
    else:
        executable = args.prebuilt
        if digest(executable) != args.prebuilt_sha256:
            raise ValueError("prebuilt compiler digest differs from its build receipt")
    work = args.work
    work.mkdir()
    compiler = Path(shutil.copy2(executable, work / "oddlang"))
    if args.prebuilt is not None and digest(compiler) != args.prebuilt_sha256:
        raise ValueError("staged compiler changed during copy")
    assembly = lane("workspace")
    inputs = work / "inputs"
    inputs.mkdir()
    spec = assembly["stage_workspace"](MANIFEST, inputs)
    assembly["stage_workspace"](PORTABLE / (BASE + ".json"), inputs)
    staged = {p.relative_to(inputs).as_posix(): digest(p) for p in sorted(inputs.rglob("*")) if p.is_file()}
    if any(Path(name).suffix not in {".json", ".ttl", ".lock"} for name in staged):
        raise ValueError("assembly inputs must be graph, overlay and lock data")
    cache = work / "providers"
    providers = assembly["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
    compiler_inputs = args.compiler_target if args.prebuilt is None else args.prebuilt
    prefix, env = JOURNAL["sandbox"](assembly, work, (ROOT, args.original, args.framework, compiler_inputs, scratch), cache)
    denied = [
        args.original / ENGINE_DATA["SOURCE"], args.original / STATUS_DATA["SOURCE"], args.framework / "src/lib.rs",
        ENGINE_DATA["HERE"] / "probe.rs", STATUS_DATA["PROBE"], REASON["PROBE"],
    ]
    for path in denied:
        proc = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if proc.returncode != 1 or proc.stdout or not any(
            msg in proc.stderr for msg in (b"Operation not permitted", b"No such file or directory")
        ):
            raise ValueError(f"source isolation failed to deny {path}")

    def invoke(*argv, cwd=work):
        command = list(prefix)
        if sys.platform == "linux":
            command[command.index("--chdir") + 1] = str(cwd)
        return run(*command, *argv, cwd=cwd, env=env)

    baseline, product = work / "canonical", work / "candidate"
    for manifest, requested in ((BASE + ".json", baseline), (MANIFEST.name, product)):
        invoke(compiler, "build", "native-workspace", "rust", inputs / manifest, requested)
    inherited = snapshots(baseline, product, args.input_revision)
    target = work / "target"
    raw = invoke("cargo", "build", "--workspace", "--lib", "--release", "--target", host,
                 "--locked", "--offline", "--target-dir", target, "--message-format=json", cwd=product)
    roles, links = AUDIT.artifacts(raw, product, list(inherited["memberLocks"]), target, generated=True)
    files = PREVIOUS["closure"](raw, target) if host == LINUX else {
        Path(file).relative_to(target).as_posix(): digest(Path(file))
        for line in raw.splitlines() if (row := json.loads(line))["reason"] == "compiler-artifact"
        for file in row["filenames"] if Path(file).suffix in {".rlib", ".rmeta", ".dylib"}
    }
    imports = import_witness(host, roles, args.original, scratch)
    observed = domains(host, raw, target, roles, links, scratch)
    if inherited != snapshots(baseline, product, args.input_revision) or any(
        digest(target / file) != checksum for file, checksum in files.items()
    ):
        raise ValueError("sealed package or Cargo library closure changed while probing")
    if source != PREVIOUS["source_pin"](args.original):
        raise ValueError("pinned original inputs changed")
    (work / "cargo-artifacts.jsonl").write_bytes(raw)
    return {
        "format": "marmish-canonical-public-data-v1", "host": host, "rustc": version,
        "framework": FRAMEWORK, "companion": args.input_revision, "original": source,
        "compilerSha256": digest(compiler), "candidateManifestSha256": digest(MANIFEST),
        "artifactLogSha256": hashlib.sha256(raw).hexdigest(),
        "cargoLibraryClosureEntries": len(files),
        "cargoLibraryClosureSha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
        "sourceDenials": len(denied), "gitProviders": providers, "stagedInputs": staged,
        "workspace": inherited, "imports": imports, "publicData": observed,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("input_revision")
    for name in ("framework", "original", "compiler_target", "work", "receipt"):
        p.add_argument(name, type=lambda value: Path(value).resolve())
    p.add_argument("--prebuilt", type=lambda value: Path(value).resolve())
    p.add_argument("--prebuilt-sha256")
    args = p.parse_args()
    if (args.prebuilt is None) != (args.prebuilt_sha256 is None):
        p.error("--prebuilt and --prebuilt-sha256 are required together")
    with tempfile.TemporaryDirectory(prefix="odd-combined-data-callers-") as temp:
        result = qualify(args, Path(temp).resolve())
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
