"""Replay shared endpoint cases on pinned original and retained generated Rust engines."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
ROOT = WHOLE.parents[1]
REFERENCE = runpy.run_path(str(WHOLE / "gap-range-reference/observe.py"))
sys.path.insert(0, str(WHOLE / "canonical-recovered-range"))
try:
    RECOVERED = runpy.run_path(str(WHOLE / "canonical-recovered-range/verify.py"))
finally:
    sys.path.pop(0)
PRIOR, AUDIT, COMBINED = (RECOVERED[key] for key in ("PRIOR", "AUDIT", "COMBINED"))
sha = REFERENCE["sha"]
PACKAGE = "marmish-engine"


def call(artifact_log, engine, target, host, work, vectors, env):
    release = target / host / "release"
    rows = [json.loads(line) for line in artifact_log.splitlines()]
    serde = COMBINED["dependency"](rows, "serde_json", release)
    libs = {"marmish_engine": engine, "serde_json": serde}
    hashes = {name: sha(lib) for name, lib in libs.items()}
    links = sorted({link for row in rows if row["reason"] == "build-script-executed"
                    for link in row["linked_paths"]})
    args = ["rustc", "+1.97.1", "--edition=2024", "--target", host, "--cfg", "with_range"]
    for name, lib in libs.items():
        args.extend(("--extern", f"{name}={lib}"))
    for link in [f"dependency={release / 'deps'}", f"dependency={target / 'release/deps'}", *links]:
        args.extend(("-L", link))
    executable = work / "endpoint-probe"
    probe = WHOLE / "gap-range-reference/probe.rs"
    subprocess.run([*args, probe, "-o", executable], cwd=work, env=env, check=True)
    result = subprocess.check_output([executable], cwd=work, env=env, input=vectors, timeout=60)
    if hashes != {name: sha(lib) for name, lib in libs.items()}:
        raise ValueError("selected libraries changed during endpoint observation")
    return {"librarySha256": hashes, "results": json.loads(result)}


def verify(args):
    original, original_target, retained = args.original, args.original_target, args.retained
    REFERENCE["pinned"](original)
    if original_target.is_relative_to(original) or original_target == retained / "target":
        raise ValueError("expected independent original Cargo target")
    env = {key: os.environ[key] for key in ("HOME", "PATH", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "SDKROOT")
           if key in os.environ}
    version = subprocess.check_output(["rustc", "+1.97.1", "-vV"], env=env, text=True).splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host not in (RECOVERED["DARWIN"], RECOVERED["LINUX"]) or (
        f"commit-hash: {REFERENCE['COMPILER']}" not in version
    ):
        raise ValueError("requires the pinned native ARM64 toolchain")

    source = original / "crates/marmish-engine/src/model.rs"
    source_sha = sha(source)
    original_path = WHOLE / "gap-range-reference" / f"original-{host}.json"
    original_witness = json.loads(original_path.read_text())
    if source_sha != original_witness["modelSha256"] or sha(original / "Cargo.lock") != original_witness["cargoLockSha256"]:
        raise ValueError("original model and Cargo inputs differ from the public source reference")
    raw = subprocess.check_output(
        ["cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--lib", "-p", PACKAGE,
         "--target", host, "--target-dir", original_target, "--message-format=json"], cwd=original, env=env,
    )
    original_engine, = [
        Path(file) for line in raw.splitlines() if (row := json.loads(line))["reason"] == "compiler-artifact"
        and row["target"]["name"] == "marmish_engine"
        and row["target"]["kind"] == ["lib"] and Path(row["manifest_path"]).resolve() == (
            original / "crates/marmish-engine/Cargo.toml"
        ) for file in row["filenames"] if file.endswith(".rlib") and Path(file).is_relative_to(original_target / host)
    ]

    old_path = WHOLE / "canonical-range-data" / f"{host}.json"
    new_path = WHOLE / "canonical-recovered-range" / f"{host}.json"
    old, new = (json.loads(path.read_text()) for path in (old_path, new_path))
    product, target = retained / "candidate", retained / "target"
    candidate = AUDIT.Candidate(RECOVERED["BASE"], old["companion"], old["framework"],
                                old["workspace"]["workspaceLockSha256"])
    AUDIT.verify_product(product, ROOT, candidate)
    if new["productComparison"]["changedLocks"]["workspace.lock"]["beforeSha256"] != candidate.lock_sha256:
        raise ValueError("the retained product is not the predecessor authenticated by the latest candidate")
    generated_log = (retained / "cargo-artifacts.jsonl").read_bytes()
    if hashlib.sha256(generated_log).hexdigest() != old["cargoArtifactLogSha256"]:
        raise ValueError("wrong retained build log")
    names = json.loads((product / "workspace.lock").read_text())["members"]
    roles, _ = AUDIT.artifacts(generated_log, product, list(names), target, generated=True)
    libraries = PRIOR["library_closure"](generated_log, target, host)
    if len(libraries) != old["cargoLibraryClosureEntries"] or (
        RECOVERED["hashed"](libraries) != old["cargoLibraryClosureSha256"]
    ):
        raise ValueError("wrong retained native build closure")

    cases = json.loads((HERE / "cases.json").read_text())
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate case IDs")
    vectors = json.dumps([{"id": case["id"], "wire": case["wire"], "type": "range"} for case in cases]).encode()
    with tempfile.TemporaryDirectory(prefix="odd-gap-endpoint-") as tmp:
        work = Path(tmp)
        reference = call(raw, original_engine, original_target, host, work, vectors, env)
        generated = call(generated_log, roles[PACKAGE][1], target, host, work, vectors, env)
    if reference["results"] != generated["results"] or [row["id"] for row in reference["results"]] != ids:
        raise ValueError("original and generated native observations differ")
    for case, row in zip(cases, reference["results"], strict=True):
        expected = case["expected"]
        result = row["result"]
        if expected is None:
            if set(result) != {"diagnostic", "line", "column"}:
                raise ValueError(f"expected native rejection for {case['id']}")
        elif result != {"encoded": expected}:
            raise ValueError(f"wrong encoded bytes for {case['id']}")
    REFERENCE["pinned"](original)
    AUDIT.verify_product(product, ROOT, candidate)
    if sha(source) != source_sha or libraries != PRIOR["library_closure"](generated_log, target, host):
        raise ValueError("source or retained generated build changed during the observations")
    return {
        "host": host, "rustc": version,
        "contract": "urn:marmish:engine:output-gap:range:end",
        "casesSha256": sha(HERE / "cases.json"), "probeSha256": sha(WHOLE / "gap-range-reference/probe.rs"),
        "observerSha256": sha(HERE / "native.py"),
        "originalSource": REFERENCE["ORIGINAL"], "originalModelSha256": source_sha,
        "originalCargoLockSha256": sha(original / "Cargo.lock"), "original": reference,
        "generatedWorkspaceLockSha256": candidate.lock_sha256, "generatedBuildReceiptSha256": sha(old_path),
        "sourceEquivalenceReceiptSha256": sha(new_path), "generated": generated,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "original_target", "retained", "out"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    args.out.write_text(json.dumps(verify(args), indent=2, sort_keys=True) + "\n")
