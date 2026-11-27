"""Qualify the appended range record and inherited public data from one sealed build."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

from compose import AFTER, BEFORE, ENGINE, PORTABLE, compose

HERE = Path(__file__).resolve().parent
ROOT, WHOLE = HERE.parents[2], HERE.parent
FRAMEWORK = "7855c055dc1e72b49d3654f6e6c500a0ffdb57ec"
DARWIN_BINARY = "5701bea86331ac273a213f9af52a64f5d8bf900b1b8cf7687e700bf2329a91ae"
MANIFEST = PORTABLE / f"{AFTER}.json"
LOCK = PORTABLE / AFTER / "workspace.lock"


def inherited():
    path = list(sys.path)
    prior = sys.modules.pop("compose")
    sys.path.insert(0, str(WHOLE / "canonical-public-data"))
    try:
        return runpy.run_path(str(WHOLE / "canonical-public-data/workspace_compat.py"))
    finally:
        sys.path[:] = path
        sys.modules["compose"] = prior


COMBINED = inherited()
AUDIT, JOURNAL, run, data, sha = (COMBINED[x] for x in ("AUDIT", "JOURNAL", "run", "data", "digest"))
ASSEMBLY = COMBINED["lane"]("workspace")
RANGE = runpy.run_path(str(WHOLE / "gap-range-record/verify.py"))
DARWIN, LINUX = COMBINED["DARWIN"], COMBINED["LINUX"]


def snapshots(base, product, prior_receipt, pin):
    old = AUDIT.Candidate(BEFORE, prior_receipt["companion"], prior_receipt["framework"],
                          prior_receipt["workspace"]["candidateWorkspaceLockSha256"])
    new = AUDIT.Candidate(AFTER, pin, FRAMEWORK, sha(LOCK))
    AUDIT.verify_product(base, ROOT, old)
    AUDIT.verify_product(product, ROOT, new)
    first, last = (data(directory / "workspace.lock") for directory in (base, product))
    if {k: v for k, v in first.items() if k not in ("inputSha256", "members")} != {
        k: v for k, v in last.items() if k not in ("inputSha256", "members")
    } or last["members"].keys() != first["members"].keys():
        raise ValueError("root workspace configuration changed")
    for name, row in last["members"].items():
        before = first["members"][name]
        expected = {"inputSha256", "standalonePackageLockSha256"} if name == ENGINE else set()
        if {key for key in row if row[key] != before[key]} != expected:
            raise ValueError(f"unexpected workspace member changes: {name}")
    original, successor = base / ENGINE, product / ENGINE
    previous, modified = (data(p / "package.lock") for p in (original, successor))
    range_package = data(PORTABLE / "engine-gap-range-v1/package.lock")
    range_component = range_package["components"][-1]
    current = modified["components"][-1]
    if modified["components"][:-1] != previous["components"] or current != range_component:
        raise ValueError("range projection or predecessor components differ from qualification")
    fields = {"manifestSha256", "generatedFiles", "components"}
    if {k: v for k, v in previous.items() if k not in fields} != {
        k: v for k, v in modified.items() if k not in fields
    }:
        raise ValueError("engine Cargo or package lock configuration changed")
    change, add = COMBINED["changed_files"](original, successor)
    idx = len(previous["components"])
    if change != {"src/lib.rs", "src/group_1/mod.rs"} or add != {f"src/group_1/__projection_{idx}.rs"}:
        raise ValueError("unexpected engine source attribution")
    for path, suffix in (
        ("src/lib.rs", "pub use model::{MissingRange};\n"),
        ("src/group_1/mod.rs", f"mod __projection_{idx};\npub use __projection_{idx}::{{MissingRange}};\n"),
    ):
        if (successor / path).read_bytes() != (original / path).read_bytes() + suffix.encode():
            raise ValueError("unexpected engine export changes")
    return {
        "baselineWorkspaceLockSha256": old.lock_sha256, "workspaceLockSha256": new.lock_sha256,
        "rootCargoLockSha256": last["cargoLockSha256"], "unchangedMemberLocks": sorted(set(last["members"]) - {ENGINE}),
        "engineMember": last["members"][ENGINE], "engineChangedFiles": sorted(change), "engineAddedFiles": sorted(add),
        "rangeProjection": current["projection"],
    }


def range_witness(original, host, raw, roles, links, target, scratch):
    scratch.mkdir()
    engine = roles[ENGINE]
    positive = AUDIT.compiler_probe(scratch, engine, "pub use observed::MissingRange as Witness;", target=host)
    hidden = AUDIT.compiler_probe(scratch, engine, "pub use observed::model::MissingRange as Witness;", target=host)
    if not positive["compiles"] or hidden["compiles"] or [row["code"] for row in hidden["diagnostics"]] != ["E0603"]:
        raise ValueError("unexpected public range export")
    release = target / host / "release"
    serde_json = COMBINED["dependency"]([json.loads(line) for line in raw.splitlines()], "serde_json", release)
    reference = RANGE["BASELINE"]
    argv = ["rustc", "+1.97.1", "--edition=2024", "--target", host, "--cfg", "with_range",
            "--extern", f"marmish_engine={engine[1]}", "--extern", f"serde_json={serde_json}",
            "-L", f"dependency={release / 'deps'}", "-L", f"dependency={target / 'release/deps'}"]
    argv.extend(arg for path in links for arg in ("-L", path))
    probe, traits = scratch / "probe", scratch / "traits"
    run(*argv, reference / "probe.rs", "-o", probe)
    run(*argv, RANGE["HERE"] / "traits.rs", "-o", traits)
    observed = json.loads(subprocess.check_output([probe], input=(reference / "cases.json").read_bytes(),
                                                 cwd=scratch, timeout=60))
    trait_output = run(traits, cwd=scratch)
    expected, reference_sha = RANGE["fixture_reference"](original, host)
    standalone = RANGE["HERE"] / f"{host}.json"
    standalone_receipt = data(standalone)["baseline"]
    case_ids = [row["id"] for row in data(reference / "cases.json") if row["type"] == "range"]
    range_rows = [row for row in observed if row["id"] in case_ids]
    if len(case_ids) != 18 or [row["id"] for row in range_rows] != case_ids or observed != expected or (
        observed != standalone_receipt["observations"] or trait_output != RANGE["TRAIT_OUTPUT"]
    ):
        raise ValueError("combined range data differs from independent native qualification")
    return {
        "publicImport": positive, "privateModuleControl": hidden,
        "rangeCases": range_rows, "pairedObservations": observed, "traits": trait_output.decode(),
        "originalReferenceSha256": reference_sha, "standaloneReceiptSha256": sha(standalone),
        "engineLibrarySha256": sha(engine[1]),
    }


def library_closure(raw, target, host):
    if host == LINUX:
        return COMBINED["PREVIOUS"]["closure"](raw, target)
    return {
        Path(path).relative_to(target).as_posix(): sha(Path(path))
        for line in raw.splitlines() if (row := json.loads(line))["reason"] == "compiler-artifact"
        for path in row["filenames"] if Path(path).suffix in {".rlib", ".rmeta", ".dylib"}
    }


def qualify(args, scratch):
    for path, revision in ((args.framework, FRAMEWORK), (args.original, AUDIT.ORIGINAL), (ROOT, args.input_revision)):
        COMBINED["ENGINE_DATA"]["checkout"](path, revision)
    compose(check=True)
    rustc = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in rustc if line.startswith("host: ")]
    if host not in {DARWIN, LINUX} or f"commit-hash: {AUDIT.COMPILER}" not in rustc:
        raise ValueError("requires the pinned native ARM64 toolchain")
    origin = COMBINED["PREVIOUS"]["source_pin"](args.original)
    if any(args.work.is_relative_to(path) for path in (ROOT, args.framework, args.original, args.baseline)):
        raise ValueError("expected independent candidate output directory")
    if args.darwin_compiler is None:
        run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
            "--target-dir", args.compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
        source = args.compiler_target / "debug/oddlang"
    else:
        source = args.darwin_compiler
        if host != DARWIN or sha(source) != DARWIN_BINARY:
            raise ValueError("wrong reviewed Darwin compiler")
    args.work.mkdir()
    compiler = Path(shutil.copy2(source, args.work / "oddlang"))
    if sha(compiler) != sha(source):
        raise ValueError("staged compiler digest changed")
    inputs = args.work / "inputs"
    inputs.mkdir()
    spec = ASSEMBLY["stage_workspace"](MANIFEST, inputs)
    staged = {p.relative_to(inputs).as_posix(): sha(p) for p in sorted(inputs.rglob("*")) if p.is_file()}
    if any(Path(name).suffix not in {".json", ".ttl", ".lock"} for name in staged):
        raise ValueError("source file in staged inputs")
    cache = args.work / "providers"
    providers = ASSEMBLY["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
    prefix, env = JOURNAL["sandbox"](ASSEMBLY, args.work,
        (ROOT, args.framework, args.original, args.baseline, source, scratch), cache)
    denied = [args.original / RANGE["MODEL"], args.framework / "src/lib.rs",
              args.baseline / ENGINE / "src/lib.rs", RANGE["BASELINE"] / "probe.rs", RANGE["HERE"] / "traits.rs"]
    for path in denied:
        proc = subprocess.run([*prefix, "/bin/cat", path], env=env, cwd=args.work, capture_output=True)
        if proc.returncode != 1 or proc.stdout or not any(
            msg in proc.stderr for msg in (b"No such file", b"Operation not permitted")
        ):
            raise ValueError(f"source containment failed to deny {path}")

    def invoke(*command, cwd=args.work):
        sandbox = list(prefix)
        if host == LINUX:
            sandbox[sandbox.index("--chdir") + 1] = str(cwd)
        return run(*sandbox, *command, cwd=cwd, env=env)

    prior_path = WHOLE / "canonical-public-data" / f"{host}.json"
    prior = data(prior_path)
    product, target = args.work / "candidate", args.work / "target"
    invoke(compiler, "build", "native-workspace", "rust", inputs / MANIFEST.name, product)
    before = snapshots(args.baseline, product, prior, args.input_revision)
    raw = invoke("cargo", "build", "--workspace", "--lib", "--release", "--target", host,
                 "--locked", "--offline", "--target-dir", target, "--message-format=json", cwd=product)
    (args.work / "cargo-artifacts.jsonl").write_bytes(raw)
    members = data(product / "workspace.lock")["members"]
    roles, links = AUDIT.artifacts(raw, product, list(members), target, generated=True)
    closure = library_closure(raw, target, host)
    recovered = range_witness(args.original, host, raw, roles, links, target, scratch / "range")
    inherited = COMBINED["domains"](host, raw, target, roles, links, scratch)
    if before != snapshots(args.baseline, product, prior, args.input_revision) or (
        closure != library_closure(raw, target, host) or origin != COMBINED["PREVIOUS"]["source_pin"](args.original)
    ):
        raise ValueError("source, generated workspace or build artifacts changed during the observations")
    return {
        "format": "marmish-canonical-range-data-v1", "host": host, "rustc": rustc,
        "original": origin, "framework": FRAMEWORK, "companion": args.input_revision, "compilerSha256": sha(compiler),
        "manifestSha256": sha(MANIFEST), "priorReceiptSha256": sha(prior_path), "workspace": before,
        "sourceDenials": len(denied), "gitProviders": providers, "stagedInputs": staged,
        "cargoArtifactLogSha256": hashlib.sha256(raw).hexdigest(), "cargoLibraryClosureEntries": len(closure),
        "cargoLibraryClosureSha256": hashlib.sha256(json.dumps(closure, sort_keys=True).encode()).hexdigest(),
        "missingRange": recovered, "inheritedPublicData": inherited,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for arg in ("framework", "original", "baseline", "compiler_target", "work", "receipt"):
        parser.add_argument(arg, type=lambda p: Path(p).resolve())
    parser.add_argument("--darwin-compiler", type=lambda p: Path(p).resolve())
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="odd-range-data-callers-") as tmp:
        result = qualify(args, Path(tmp).resolve())
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
