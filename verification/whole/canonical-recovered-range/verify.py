"""Compare the complete source-recovered product against the sealed range product."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

from compose import AUTHORED, BASE, ENGINE, OUTPUT, PORTABLE, RANGE, REASON, compose

HERE = Path(__file__).resolve().parent
ROOT, WHOLE = HERE.parents[2], HERE.parent
MANIFEST = PORTABLE / f"{OUTPUT}.json"
LOCK = PORTABLE / OUTPUT / "workspace.lock"
FRAMEWORK = "9923e6664899de66915ed3a94e417f261ffed401"


def inherit():
    paths = list(sys.path)
    prior = sys.modules.pop("compose")
    directory = WHOLE / "canonical-range-data"
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / "verify.py"))
    finally:
        sys.path[:] = paths
        sys.modules["compose"] = prior


PRIOR = inherit()
COMBINED, AUDIT, JOURNAL, ASSEMBLY, run, data, sha = (
    PRIOR[name] for name in ("COMBINED", "AUDIT", "JOURNAL", "ASSEMBLY", "run", "data", "sha")
)
DARWIN, LINUX = PRIOR["DARWIN"], PRIOR["LINUX"]
ENGINE_PACKAGE_LOCK = f"{ENGINE}/package.lock"
EXPECTED_LOCK_DELTAS = {
    "workspace.lock": {
        "inputSha256", f"members/{ENGINE}/inputSha256", f"members/{ENGINE}/standalonePackageLockSha256",
    },
    ENGINE_PACKAGE_LOCK: {
        "manifestSha256", "components/5/graphSha256", "components/5/overlaySha256",
        "components/5/projection/graphSha256", "components/5/projection/overlaySha256",
    },
}


def hashed(data):
    return hashlib.sha256(json.dumps(data, sort_keys=True).encode()).hexdigest()


def lock_delta(old, new, path=""):
    if old == new:
        return {}
    if isinstance(old, dict) and isinstance(new, dict) and old.keys() == new.keys():
        children = [(key, old[key], new[key]) for key in sorted(old)]
    elif isinstance(old, list) and isinstance(new, list) and len(old) == len(new):
        children = [(str(i), a, b) for i, (a, b) in enumerate(zip(old, new, strict=True))]
    else:
        return {path: {"before": old, "after": new}}
    return {key: delta for name, a, b in children
            for key, delta in lock_delta(a, b, f"{path}/{name}".lstrip("/")).items()}


def tree(root):
    result = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            raise ValueError(f"unexpected symbolic link in product: {relative}")
        if path.is_dir():
            result[relative] = {"kind": "directory"}
        elif path.is_file():
            result[relative] = {"kind": "file", "sha256": sha(path)}
        else:
            raise ValueError(f"unexpected file kind: {relative}")
    return result


def compare(before, after):
    old, new = tree(before), tree(after)
    if old.keys() != new.keys():
        raise ValueError("emitted directory or file set changed")
    changed = {name for name in old if old[name] != new[name]}
    if changed != EXPECTED_LOCK_DELTAS.keys():
        raise ValueError(f"unexpected emitted file changes: {sorted(changed)}")
    locks = {}
    for name, expected in EXPECTED_LOCK_DELTAS.items():
        delta = lock_delta(data(before / name), data(after / name))
        if delta.keys() != expected:
            raise ValueError(f"unexpected changed lock fields in {name}: {sorted(delta)}")
        locks[name] = {"beforeSha256": old[name]["sha256"], "afterSha256": new[name]["sha256"], "pins": delta}
    same = {name: row for name, row in old.items() if name not in changed}
    for name, row in same.items():
        if row["kind"] == "file" and (before / name).read_bytes() != (after / name).read_bytes():
            raise ValueError(f"inherited file bytes changed: {name}")
    rust = {name: row["sha256"] for name, row in same.items() if name.endswith(".rs")}
    return {
        "baselineTreeSha256": hashed(old), "recoveredTreeSha256": hashed(new),
        "commonTreeSha256": hashed(same), "directories": sum(row["kind"] == "directory" for row in old.values()),
        "files": sum(row["kind"] == "file" for row in old.values()), "identicalRustFiles": rust, "changedLocks": locks,
    }


def verify(args, scratch):
    for path, pin in ((ROOT, args.input_revision), (args.framework, FRAMEWORK), (args.original, AUDIT.ORIGINAL)):
        COMBINED["ENGINE_DATA"]["checkout"](path, pin)
    if any(args.work.is_relative_to(path) for path in (ROOT, args.framework, args.original, args.prior_work)):
        raise ValueError("choose an independent assembly area")
    compose(check=True)
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host not in (DARWIN, LINUX) or f"commit-hash: {AUDIT.COMPILER}" not in version:
        raise ValueError("requires the pinned native ARM64 toolchain")
    recovery_path = WHOLE / "gap-range-recovery" / host / "receipt.json"
    recovery = data(recovery_path)
    if recovery["framework"] != FRAMEWORK or recovery["original"] != AUDIT.ORIGINAL or (
        sha(args.compiler) != recovery["compilerSha256"]
    ):
        raise ValueError("recovered-input producer pin differs")
    reason_path = WHOLE / "gap-reason-recovery" / f"{host}.json"
    reason_receipt = data(reason_path)
    reason = reason_receipt["baselineRecovery"]
    recovered_range = recovery["cases"]["baseline"]
    model = args.original / PRIOR["RANGE"]["MODEL"]
    if reason_receipt["original"] != AUDIT.ORIGINAL or sha(model) != reason["sourceSha256"] or (
        sha(model.with_name("lib.rs")) != recovery["crateRootSha256"]
    ):
        raise ValueError("source-recovery receipts differ from the pinned original")
    for folder, evidence in ((REASON, reason), (RANGE, recovered_range)):
        for key, name in (("graphSha256", "graph.json"), ("overlaySha256", "rust.json")):
            if sha(PORTABLE / folder / name) != evidence[key]:
                raise ValueError(f"published {folder}/{name} differs from the qualification receipt")
    prior_path = WHOLE / "canonical-range-data" / f"{host}.json"
    prior = data(prior_path)
    if recovered_range["observations"] != prior["missingRange"]["pairedObservations"]:
        raise ValueError("recovered range and pinned #123 native behaviors differ")
    old = AUDIT.Candidate(BASE, prior["companion"], prior["framework"], prior["workspace"]["workspaceLockSha256"])
    new = AUDIT.Candidate(OUTPUT, args.input_revision, FRAMEWORK, sha(LOCK))
    baseline, target = args.prior_work / "candidate", args.prior_work / "target"
    AUDIT.verify_product(baseline, ROOT, old)
    artifacts = (args.prior_work / "cargo-artifacts.jsonl").read_bytes()
    if hashlib.sha256(artifacts).hexdigest() != prior["cargoArtifactLogSha256"]:
        raise ValueError("wrong archived #123 Cargo artifact log")
    libraries = PRIOR["library_closure"](artifacts, target, host)
    if len(libraries) != prior["cargoLibraryClosureEntries"] or hashed(libraries) != prior["cargoLibraryClosureSha256"]:
        raise ValueError("retained #123 native build closure differs")

    work = args.work
    work.mkdir()
    inputs = work / "inputs"
    inputs.mkdir()
    spec = ASSEMBLY["stage_workspace"](MANIFEST, inputs)
    staged = {p.relative_to(inputs).as_posix(): sha(p) for p in sorted(inputs.rglob("*")) if p.is_file()}
    required = {f"{folder}/{file}" for folder in (REASON, RANGE) for file in ("graph.json", "rust.json")}
    if required - staged.keys() or any(Path(path).parts[0] in AUTHORED for path in staged) or any(
        Path(path).suffix not in (".json", ".ttl", ".lock") for path in staged
    ):
        raise ValueError("authored fallback or missing recovered graph in sealed stage")
    compiler = Path(shutil.copy2(args.compiler, work / "oddlang"))
    if sha(compiler) != recovery["compilerSha256"]:
        raise ValueError("staged compiler digest differs")
    cache = work / "providers"
    providers = ASSEMBLY["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
    prefix, env = JOURNAL["sandbox"](ASSEMBLY, work,
        (ROOT, args.original, args.framework, args.prior_work, args.compiler, scratch), cache)
    denied = {
        "original-model": args.original / PRIOR["RANGE"]["MODEL"],
        "framework-source": args.framework / "src/lib.rs",
        "baseline-engine": baseline / ENGINE / "src/lib.rs",
        "independent-range-caller": PRIOR["RANGE"]["BASELINE"] / "probe.rs",
        "authored-missing-reason-graph": PORTABLE / "engine-gap-reason-v1/graph.json",
        "authored-missing-reason-overlay": PORTABLE / "engine-gap-reason-v1/rust.json",
        "authored-missing-range-graph": PORTABLE / "engine-gap-range-v1/graph.json",
        "authored-missing-range-overlay": PORTABLE / "engine-gap-range-v1/rust.json",
    }
    for label, path in denied.items():
        if not path.is_file():
            raise ValueError(f"missing denial witness: {label}")
        status = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if status.returncode != 1 or status.stdout or not any(
            msg in status.stderr for msg in (b"Operation not permitted", b"No such file or directory")
        ):
            raise ValueError(f"generation sandbox did not deny {label}")
    candidate = work / "candidate"
    run(*prefix, compiler, "build", "native-workspace", "rust", inputs / MANIFEST.name, candidate, cwd=work, env=env)
    AUDIT.verify_product(candidate, ROOT, new)
    comparison = compare(baseline, candidate)
    leaf = f"{ENGINE}/src/group_1/__projection_5.rs"
    if comparison["identicalRustFiles"][leaf] != recovered_range["generatedFiles"]["src/group_0/__projection_1.rs"]:
        raise ValueError("combined Rust range differs from the native recovered-input qualification")
    if libraries != PRIOR["library_closure"](artifacts, target, host):
        raise ValueError("the retained #123 native behavior witness changed")
    return {
        "format": "marmish-canonical-recovered-range-equivalence-v1", "host": host,
        "rustc": version, "framework": FRAMEWORK, "compilerSha256": sha(compiler),
        "companion": args.input_revision, "original": AUDIT.ORIGINAL, "manifestSha256": sha(MANIFEST),
        "workspaceLockSha256": new.lock_sha256, "priorWorkspaceLockSha256": old.lock_sha256,
        "rangeRecoveryReceiptSha256": sha(recovery_path), "reasonRecoveryReceiptSha256": sha(reason_path),
        "priorNativeReceiptSha256": sha(prior_path),
        "priorCargoArtifactLogSha256": prior["cargoArtifactLogSha256"],
        "priorCargoLibraryClosureEntries": len(libraries), "priorCargoLibraryClosureSha256": hashed(libraries),
        "sourceDenials": sorted(denied), "gitProviders": providers, "stagedInputs": staged, "productComparison": comparison,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("input_revision")
    for param in ("framework", "compiler", "original", "prior_work", "work", "receipt"):
        p.add_argument(param, type=lambda value: Path(value).resolve())
    args = p.parse_args()
    with tempfile.TemporaryDirectory(prefix="odd-recovered-range-compare-") as scratch:
        receipt = verify(args, Path(scratch).resolve())
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
