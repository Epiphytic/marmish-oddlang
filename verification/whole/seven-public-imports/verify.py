"""Audit upstream public imports against the seven generated release libraries."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
INVENTORY = HERE.parent / "public-types"
RELEASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered"
INPUT_REVISION = "6a3eb779b60e63bef3dbfbb83e64b8a486ef77df"
RUNTIME = "59f947dafdb4cf3055e35216857472ada895a76a"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HOST = "aarch64-apple-darwin"
WORKSPACE_LOCK = "fdfa5b16d9172fe97705ee5794251c6168d0e226ea159334a52286e2e7172b68"
COUNTS = {"core": 6, "engine": 48, "mdk": 5, "native": 36, "prototype": 0, "stream": 35, "vault": 10}
IMPORT_ERRORS = {"E0432", "E0433", "E0603", "ambiguous_glob_imports"}

if not __debug__:
    raise RuntimeError("verification requires Python assertions")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def run(argv, cwd, *, check=True):
    environment = {k: v for k, v in os.environ.items() if not k.startswith(
        ("RUST", "CARGO_BUILD_", "CARGO_ENCODED_", "CARGO_PROFILE_", "CARGO_TARGET_")
    )}
    call = subprocess.run(argv, cwd=cwd, env=dict(environment, CARGO_NET_OFFLINE="true"),
                          capture_output=True, check=False)
    if check and call.returncode:
        raise RuntimeError(call.stderr.decode(errors="replace"))
    return call


def git(root, *args):
    return run(["git", "-C", root, *args], root).stdout


def verify_product(product, inputs):
    portable = inputs / "oddlang/portable"
    release = portable / f"{RELEASE}.json"
    for path in (release, portable / RELEASE / "workspace.lock"):
        assert path.read_bytes() == git(inputs, "show", f"{INPUT_REVISION}:{path.relative_to(inputs)}")
    locked = (product / "workspace.lock").read_bytes()
    assert sha(locked) == WORKSPACE_LOCK
    assert locked == (portable / RELEASE / "workspace.lock").read_bytes()
    workspace = json.loads(locked)
    assert sha(release.read_bytes()) == workspace["inputSha256"]
    assert set(workspace["members"]) == {f"marmish-{name}" for name in COUNTS}
    assert set(p.name for p in product.iterdir()) == set(workspace["members"]) | {
        "Cargo.toml", "Cargo.lock", "workspace.lock"}
    assert not any(path.is_symlink() for path in product.rglob("*"))
    assert sha((product / "Cargo.toml").read_bytes()) == workspace["workspaceCargoManifestSha256"]
    assert sha((product / "Cargo.lock").read_bytes()) == workspace["cargoLockSha256"]
    manifest = read(release)
    for name, member in workspace["members"].items():
        package = product / name
        pin = manifest["members"][name]["manifest"]
        assert pin["sha256"] == member["inputSha256"] == sha((portable / pin["path"]).read_bytes())
        assert sha((package / "Cargo.toml").read_bytes()) == member["workspaceCargoManifestSha256"]
        lock = package / "package.lock"
        assert sha(lock.read_bytes()) == member["standalonePackageLockSha256"]
        files = {
            p.relative_to(package).as_posix(): sha(p.read_bytes())
            for p in package.rglob("*") if p.is_file() and p not in (package / "Cargo.toml", lock)
        }
        assert files == read(lock)["generatedFiles"], name
    return manifest


def artifacts(raw, root, packages, target, *, generated):
    rows = [json.loads(line) for line in raw.splitlines()]
    assert rows[-1] == {"reason": "build-finished", "success": True}
    matches = {}
    for row in rows:
        if row["reason"] != "compiler-artifact":
            continue
        name = row["target"]["name"].replace("_", "-")
        if name not in packages or row["target"]["kind"] != ["lib"]:
            continue
        package = (root if generated else root / "crates") / name
        assert Path(row["manifest_path"]).resolve() == package / "Cargo.toml"
        assert Path(row["target"]["src_path"]).resolve().is_relative_to(package)
        assert row["profile"]["opt_level"] == "3" and not row["profile"]["test"]
        rmeta, = [Path(f).resolve() for f in row["filenames"] if f.endswith(".rmeta")]
        rlib, = [Path(f).resolve() for f in row["filenames"] if f.endswith(".rlib")]
        assert rmeta.is_relative_to(target) and rlib.is_relative_to(target)
        assert name not in matches
        matches[name] = (rmeta, rlib, row["features"], target / "release/deps", row["fresh"])
    assert matches.keys() == set(packages)
    return matches, sorted({path for row in rows if row["reason"] == "build-script-executed"
                            for path in row["linked_paths"]})


def compiler_probe(work, artifact, body, *, binary=False, link_paths=(), allowed=IMPORT_ERRORS):
    path = work / "caller.rs"
    path.write_text(body)
    command = ["rustc", "+1.97.1", "--edition", "2024", "--target", HOST,
               "--error-format=json", "--crate-name", "separate_caller",
               "--extern", f"observed={artifact[1 if binary else 0]}",
               "-L", f"dependency={artifact[0].parent}", "-L", f"dependency={artifact[3]}", "caller.rs"]
    if binary:
        command.extend(["-C", "opt-level=3", "-o", "constant"])
        command.extend(arg for link_path in link_paths for arg in ("-L", link_path))
    else:
        command.extend(["--crate-type", "lib", "--emit", "metadata"])
    result = run(command, work, check=False)
    diagnostics = [
        row for line in result.stderr.splitlines() if line.startswith(b"{")
        if (row := json.loads(line)).get("level") == "error" and row["code"]
    ]
    if result.returncode:
        assert diagnostics and {row["code"]["code"] for row in diagnostics} <= allowed, result.stderr
    else:
        assert not diagnostics
    return {"compiles": result.returncode == 0,
            "diagnostics": [{"code": row["code"]["code"], "message": row["message"],
                             "rendered": row["rendered"]} for row in diagnostics]}


def imports(name, artifact, original):
    root = INVENTORY / f"original-{name}"
    graph_path = root / "exports.json"
    lock_path = root / "exports.lock"
    graph, lock = read(graph_path), read(lock_path)
    pin = lock["pin"]
    evidence_path = root / "evidence.json"
    evidence = read(evidence_path)
    assert evidence["exportsSha256"] == lock["exportsSha256"] and not evidence["discrepancies"]
    assert evidence["compilePositiveCount"] == COUNTS[name]
    assert pin["cargoLockSha256"] == sha((original / "Cargo.lock").read_bytes())
    assert pin["packageManifestSha256"] == sha(
        (original / "crates" / f"marmish-{name}" / "Cargo.toml").read_bytes())
    assert pin["sourceRevision"] in (None, ORIGINAL)
    assert lock["exportsSha256"] == sha(graph_path.read_bytes())
    assert graph["vocabulary"] == lock["vocabulary"] == "oddlang-public-type-bindings-v1"
    assert pin["package"] == f"marmish-{name}" and pin["targetName"] == f"marmish_{name}"
    assert pin["compilerCommit"] == COMPILER and pin["targetTriple"] == HOST
    assert pin["targetKind"] == "lib" and pin["defaultFeatures"] and not pin["features"]
    paths = {}
    unresolved = []
    for module in graph["modules"].values():
        assert not module["ambiguous"]
        unresolved.extend(module["path"] + [item] for item in module["unresolved"])
        for item, binding in module["bindings"].items():
            path = tuple(module["path"] + [item])
            assert path not in paths
            paths[path] = binding["kind"]
    assert len(paths) == COUNTS[name]
    with tempfile.TemporaryDirectory(prefix=f"odd-imports-{name}-") as temp:
        work = Path(temp)
        assert compiler_probe(work, artifact, "pub use observed as subject;")["compiles"]
        rows = []
        for path, kind in sorted(paths.items()):
            spelling = "::".join(("observed", *("r#" + segment for segment in path)))
            result = compiler_probe(work, artifact,
                                    f"#![deny(ambiguous_glob_imports)]\npub use {spelling} as witness;\n")
            rows.append({"spelling": "::".join(path), "originalInventoryKind": kind, **result})
    count = sum(row["compiles"] for row in rows)
    return {
        "inventorySha256": sha(graph_path.read_bytes()), "inventoryLockSha256": sha(lock_path.read_bytes()),
        "inventoryUnresolved": sorted(unresolved), "inventoryEvidenceSha256": sha(evidence_path.read_bytes()),
        "metadataSha256": sha(artifact[0].read_bytes()), "librarySha256": sha(artifact[1].read_bytes()),
        "features": artifact[2], "cargoFresh": artifact[4], "importable": count, "missing": len(rows) - count, "imports": rows,
    }


def prototype_probe(original, candidate, original_link, candidate_link):
    constant = """const BOUND: usize = observed::MAX_LINE;
fn main() { println!("{BOUND}"); }
"""
    signature = """pub fn signature<R: std::io::Read, W: std::io::Write>()
    -> fn(R, W) -> std::io::Result<()>
{
    observed::serve
}
"""
    result = {}
    with tempfile.TemporaryDirectory(prefix="odd-prototype-surface-") as temp:
        work = Path(temp)
        for role, artifact, paths in (("original", original, original_link),
                                      ("generated", candidate, candidate_link)):
            maximum = compiler_probe(work, artifact, constant, binary=True, link_paths=paths, allowed={"E0425", "E0603"})
            if maximum["compiles"]:
                maximum["value"] = int(run([work / "constant"], work).stdout)
            result[role] = {"MAX_LINE": maximum, "serve": compiler_probe(work, artifact, signature, allowed={"E0425", "E0603", "E0308", "E0277"}),
                            "librarySha256": sha(artifact[1].read_bytes())}
    assert result["original"]["MAX_LINE"]["value"] == 131_072
    assert result["original"]["serve"]["compiles"]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("product", "candidate_target", "original", "original_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    parser.add_argument("--release-inputs", type=lambda value: Path(value).resolve(), default=ROOT)
    parser.add_argument("--candidate-artifacts", type=Path)
    parser.add_argument("--original-artifacts", type=Path)
    args = parser.parse_args()
    for target in (args.candidate_target, args.original_target):
        assert not target.is_relative_to(args.product) and not target.is_relative_to(args.original)
    assert git(args.original, "rev-parse", "HEAD").decode().strip() == ORIGINAL
    assert not git(args.original, "status", "--porcelain", "--untracked-files=no")
    version = run(["rustc", "+1.97.1", "-vV"], ROOT).stdout.decode().splitlines()
    assert f"commit-hash: {COMPILER}" in version and f"host: {HOST}" in version
    provenance = INVENTORY / "provenance.json"
    assert read(provenance)["original"]["revision"] == ORIGINAL
    manifest = verify_product(args.product, args.release_inputs)
    candidate = (args.candidate_artifacts.read_bytes() if args.candidate_artifacts else
                 run(["cargo", "+1.97.1", "build", "--release", "--workspace", "--lib", "--locked",
                      "--offline", "--target-dir", args.candidate_target, "--message-format=json"],
                     args.product).stdout)
    baseline = (args.original_artifacts.read_bytes() if args.original_artifacts else
                run(["cargo", "+1.97.1", "build", "--release", "--lib", "--package", "marmish-prototype",
                     "--target", HOST, "--locked", "--offline", "--target-dir", args.original_target,
                     "--message-format=json"], args.original).stdout)
    found, candidate_link = artifacts(candidate, args.product, manifest["members"],
                                      args.candidate_target, generated=True)
    original, original_link = artifacts(baseline, args.original, ["marmish-prototype"],
                                        args.original_target, generated=False)
    with ThreadPoolExecutor(max_workers=7) as jobs:
        result = dict(zip(COUNTS, jobs.map(lambda name: imports(name, found[f"marmish-{name}"], args.original), COUNTS)))
    probe = prototype_probe(original["marmish-prototype"], found["marmish-prototype"],
                            original_link, candidate_link)
    assert verify_product(args.product, args.release_inputs) == manifest
    report = {
        "workspaceInputRevision": INPUT_REVISION, "frameworkRevision": RUNTIME, "original": ORIGINAL,
        "workspaceLockSha256": WORKSPACE_LOCK,
        "workspaceManifestSha256": sha((args.release_inputs / "oddlang/portable" / f"{RELEASE}.json").read_bytes()),
        "upstreamInventoryProvenanceSha256": sha(provenance.read_bytes()), "rustc": version,
        "candidateArtifactLogSha256": sha(candidate), "originalArtifactLogSha256": sha(baseline),
        "verifierSha256": sha(Path(__file__).read_bytes()),
        "imports": result, "totals": {k: sum(value[k] for value in result.values())
                                       for k in ("importable", "missing")},
        "prototypeValueProbes": probe,
    }
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
