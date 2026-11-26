#!/usr/bin/env python3
"""Recover and verify the graph-only Marmish workspace data shape archive."""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

UPSTREAM = "af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK = "5ec32f8177cf9d7b4ffcce0c192c5553af7f2042"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
TOOLCHAIN = "1.97.1"
VOCABULARY = "oddlang-data-shapes-v1"
RELEASE = dict(version=1, upstreamRevision=UPSTREAM, frameworkRevision=FRAMEWORK,
               vocabulary=VOCABULARY, compilerCommit=COMPILER, cargoProfile="release")
TRIPLES = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu")
FEATURES = ("fault-injection", "installed-checks")
ADAPTER = Path("verification/rust-recovery")
INPUTS = ("Cargo.toml", "Cargo.lock", "driver.rs", "shape_model.rs", "shapes.rs")
ARCHIVE = Path(__file__).resolve().parents[3] / "oddlang/portable/workspace-shapes-v1"


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def semantic_lock_sha256(lock):
    pin_value = lock["pin"].copy()
    # Preserve host-specific Cargo config in the raw lock.
    pin_value.pop("cargoConfigSha256")
    value = {**lock, "pin": pin_value}
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"))
                          .encode("utf-8")).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def write(path, obj):
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                    encoding="utf-8")


def capture(root, *command):
    return subprocess.check_output(command, cwd=root, text=True).strip()


def checkout(root, revision):
    require(capture(root, "git", "rev-parse", "HEAD") == revision, f"wrong revision in {root}")
    require(not capture(root, "git", "status", "--porcelain", "--untracked-files=all"),
            f"expected a clean checkout at {root}")


def matrix(original):
    checkout(original, UPSTREAM)
    meta = json.loads(capture(original, "cargo", f"+{TOOLCHAIN}", "metadata", "--locked",
                              "--offline", "--format-version=1", "--no-deps"))
    targets = []
    cells = []
    members = set(meta["workspace_members"])
    for package in sorted(meta["packages"], key=lambda p: p["name"]):
        if package["id"] not in members:
            continue
        for target in sorted(package["targets"], key=lambda t: t["name"]):
            if target["kind"] not in (["lib"], ["bin"]):
                continue
            required = sorted(target.get("required-features", []))
            target_row = dict(package=package["name"], kind=target["kind"][0],
                              target=target["name"], requiredFeatures=required)
            targets.append(target_row)
            for triple in TRIPLES:
                for features in ([], *([f] for f in FEATURES if f in package["features"])):
                    selected = dict(package=package["name"], kind=target["kind"][0],
                                    target=target["name"], triple=triple, features=features,
                                    defaultFeatures=True)
                    if set(required) <= set(features) | set(package["features"].get("default", [])):
                        cells.append(selected)
    return targets, cells


def key(cell):
    return ".".join((cell["triple"], cell["package"], cell["kind"], cell["target"],
                     *cell["features"]))


def pin(original, cell):
    return dict(package=cell["package"], targetKind=cell["kind"], targetName=cell["target"],
                targetTriple=cell["triple"], features=cell["features"],
                defaultFeatures=cell["defaultFeatures"], compilerCommit=COMPILER,
                cargoLockSha256=sha(original / "Cargo.lock"),
                manifestSha256=sha(original / "Cargo.toml"),
                packageManifestSha256=sha(original / "crates" / cell["package"] / "Cargo.toml"))


def is_sha256(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def verify_lock(lock, digest, original, framework, cell):
    require(set(lock) == {"vocabulary", "pin", "shapesSha256", "qualification"}
            and lock["vocabulary"] == VOCABULARY and lock["qualification"] == "candidate"
            and lock["shapesSha256"] == digest, f"bad candidate lock for {key(cell)}")
    expected = pin(original, cell)
    expected.update(adapterSha256=sha(framework / ADAPTER / "run.sh"),
                    adapterInputsSha256={name: sha(framework / ADAPTER / name) for name in INPUTS})
    actual = lock["pin"].copy()
    # The raw lock records Cargo config fingerprints from the recovery host.
    # Their values are outside the portable pin and cannot be reverified here.
    config = actual.pop("cargoConfigSha256", None)
    require(isinstance(config, dict) and all(is_sha256(value) for value in config.values()),
            "invalid Cargo configuration")
    require(actual.pop("sourceRevision", None) == UPSTREAM,
            f"missing or invalid source revision for {key(cell)}")
    require(actual == expected, f"profile pin mismatch for {key(cell)}")


def ordered(rows, what):
    require([row["position"] for row in rows] == list(range(len(rows))),
            f"invalid {what} order")


ROOT_GAPS = {"data-layout-and-representation", "visibility-and-export-paths",
             "workspace-package-identities"}
UNRESOLVED_REASONS = {"array-length", "constant-argument", "dynamic-type", "function-pointer",
                      "lifetime-binding", "type-alias-projection", "unsupported-type"}
GAP_REASONS = ROOT_GAPS | UNRESOLVED_REASONS | {
    "compiler-disambiguated-identity", "explicit-discriminant", "external-package-instance",
    "generic-constraints-and-defaults", "raw-pointer-provenance", "union-active-field",
}
INTEGER_WIDTHS = {8, 16, 32, 64, 128}
WIDTHS = {
    "signed-integer": INTEGER_WIDTHS,
    "unsigned-integer": INTEGER_WIDTHS,
    "target-integer": {64},  # Every supported triple has 64-bit pointers.
    "floating-point": {16, 32, 64, 128},
}
TYPE_KEYS = {
    "boolean": "", "unicode-scalar": "", "text": "", "never": "", "signed-integer": "bits",
    "unsigned-integer": "bits", "target-integer": "bits signed", "floating-point": "bits",
    "named": "id arguments", "parameter": "id", "static-lifetime": "", "tuple": "elements",
    "reference": "ownership lifetime target", "raw-pointer": "mutable target",
    "slice": "element", "unresolved": "reason",
}


def keys(obj, required, optional=""):
    base = set(required.split())
    require(base <= obj.keys() <= base | set(optional.split()), "unexpected graph fields")


def type_shape(shape, owner, local_references, required_gaps):
    kind = shape["kind"]
    keys(shape, "kind " + TYPE_KEYS[kind], "ownership" if kind == "named" else "")
    if kind in WIDTHS:
        bits = shape["bits"]
        require(type(bits) is int and bits in WIDTHS[kind], f"invalid {kind} width at {owner}")
    if kind == "named":
        if shape["id"].startswith("local::"):
            local_references.add(shape["id"])
        ordered(shape["arguments"], "type argument")
        for arg in shape["arguments"]:
            keys(arg, "name parameter position value")
            type_shape(arg["value"], owner, local_references, required_gaps)
    elif kind == "tuple":
        ordered(shape["elements"], "tuple element")
        for element in shape["elements"]:
            keys(element, "position type")
            type_shape(element["type"], owner, local_references, required_gaps)
    elif kind == "reference":
        type_shape(shape["lifetime"], owner, local_references, required_gaps)
        type_shape(shape["target"], owner, local_references, required_gaps)
    elif kind == "raw-pointer":
        type_shape(shape["target"], owner, local_references, required_gaps)
    elif kind == "slice":
        type_shape(shape["element"], owner, local_references, required_gaps)
    elif kind == "unresolved":
        reason = shape["reason"]
        require(reason in UNRESOLVED_REASONS, f"unknown unresolved reason {reason}")
        required_gaps.add((owner, reason))


def coverage(graph, target):
    keys(graph, "vocabulary declarations gaps")
    require(graph["vocabulary"] == VOCABULARY, "invalid vocabulary")
    root_id = "local::" + target.replace("-", "_")
    identities = {root_id}
    local_references = set()
    required_gaps = {(root_id, reason) for reason in ROOT_GAPS}

    def add(row, owner=None):
        identity = row["id"]
        require(identity.startswith(root_id + "::"), f"foreign identity {identity}")
        require(identity not in identities, f"duplicate identity {identity}")
        identities.add(identity)
        if owner is not None:
            require(row["owner"] == owner, f"incorrect owner for {identity}")

    def fields(rows, owner):
        ordered(rows, "field")
        for field in rows:
            keys(field, "id owner name position type")
            add(field, owner)
            type_shape(field["type"], field["id"], local_references, required_gaps)

    declarations = graph["declarations"]
    declaration_ids = [row["id"] for row in declarations]
    require(declaration_ids == sorted(declaration_ids), "invalid declaration order")
    for declaration in declarations:
        kind = declaration["kind"]
        extra = {"product": "form fields", "sum": "variants", "union": "fields"}[kind]
        keys(declaration, "id name parameters kind " + extra)
        add(declaration)
        ordered(declaration["parameters"], "parameter")
        for parameter in declaration["parameters"]:
            keys(parameter, "id name position kind")
            add(parameter)
        owner = declaration["id"]
        if kind == "sum":
            ordered(declaration["variants"], "variant")
            for variant in declaration["variants"]:
                keys(variant, "id owner name position form fields")
                add(variant, owner)
                fields(variant["fields"], variant["id"])
        else:
            fields(declaration["fields"], owner)

    missing_declarations = local_references.difference(declaration_ids)
    require(not missing_declarations, f"unknown local declaration {sorted(missing_declarations)}")
    gaps = []
    for gap in graph["gaps"]:
        keys(gap, "owner reason")
        require(gap["owner"] in identities, f"unknown gap owner {gap['owner']}")
        reason = gap["reason"]
        require(reason in GAP_REASONS, f"unknown gap reason {reason}")
        owner = gap["owner"]
        require(reason not in ROOT_GAPS or owner == root_id, f"invalid root gap owner {owner}")
        require(reason not in UNRESOLVED_REASONS or (owner, reason) in required_gaps,
                f"unmatched unresolved gap at {owner}")
        gaps.append((owner, reason))
    require(gaps == sorted(set(gaps)), "invalid gap order or duplicate gap")
    missing_gaps = required_gaps.difference(gaps)
    require(not missing_gaps, f"missing required gaps {sorted(missing_gaps)}")
    return {"declarations": len(declarations), "gaps": dict(sorted(Counter(r for _, r in gaps).items()))}


def artifacts(output, original, framework, cell):
    graph = output / "shapes.json"
    lock = output / "shapes.lock"
    digest = sha(graph)
    verify_lock(read(lock), digest, original, framework, cell)
    return graph, lock, coverage(read(graph), cell["target"])


def seal(original, framework, archive, observed):
    targets, cells = matrix(original)
    graph_dir, lock_dir = (archive / "graphs", archive / "locks")
    graph_dir.mkdir(parents=True)
    lock_dir.mkdir()
    records = []
    for cell in cells:
        found = observed.get(key(cell))
        if found is None:
            records.append({**cell, "status": "unobserved"})
            continue
        graph, lock, stats = artifacts(found, original, framework, cell)
        digest = sha(graph)
        shutil.copyfile(graph, graph_dir / f"{digest}.json")
        shutil.copyfile(lock, lock_dir / f"{key(cell)}.json")
        records.append({**cell, "status": "candidate", "graphSha256": digest,
                        "lockSha256": sha(lock), "semanticLockSha256": semantic_lock_sha256(read(lock)),
                        "coverage": stats})
    write(archive / "release.json", {**RELEASE, "targets": targets, "cells": records})


def verify(original, framework, archive):
    checkout(framework, FRAMEWORK)
    required_root = {"graphs", "locks", "release.json"}
    root_entries = {path.name for path in archive.iterdir()}
    require(required_root <= root_entries <= required_root | {"README.md"},
            "invalid archive root inventory")
    manifest = read(archive / "release.json")
    targets, cells = matrix(original)
    require(manifest["targets"] == targets, "target inventory differs from Cargo metadata")
    require({k: v for k, v in manifest.items() if k not in ("targets", "cells")} == RELEASE,
            "release pin mismatch")
    require(len(manifest["cells"]) == len(cells), "missing or extra profile cells")
    require(any(record["status"] == "candidate" for record in manifest["cells"]),
            "archive contains no observations")
    graphs, locks = set(), set()
    for cell, record in zip(cells, manifest["cells"]):
        if record["status"] == "unobserved":
            require(record == {**cell, "status": "unobserved"}, "invalid unobserved cell")
            continue
        lock = archive / "locks" / f"{key(cell)}.json"
        require(sha(lock) == record["lockSha256"], f"lock digest mismatch for {key(cell)}")
        digest = record["graphSha256"]
        require(is_sha256(digest),
                f"invalid graph digest for {key(cell)}")
        graph = archive / "graphs" / f"{digest}.json"
        require(sha(graph) == digest, f"graph digest mismatch for {key(cell)}")
        lock_value = read(lock)
        verify_lock(lock_value, digest, original, framework, cell)
        require(record == {**cell, "status": "candidate", "graphSha256": digest,
                           "lockSha256": sha(lock), "semanticLockSha256": semantic_lock_sha256(lock_value),
                           "coverage": coverage(read(graph), cell["target"])},
                f"cell metadata mismatch for {key(cell)}")
        graphs.add(graph)
        locks.add(lock)
    require(graphs == set((archive / "graphs").iterdir()), "extra graph files")
    require(locks == set((archive / "locks").iterdir()), "extra lock files")


def compare(original, framework, archive, work):
    verify(original, framework, archive)
    records = {key(cell): cell for cell in read(archive / "release.json")["cells"]}
    paths = sorted(path for path in work.iterdir() if path.is_dir())
    require(paths, "no replay directories")
    for path in paths:
        record = records[path.name]
        require(record["status"] == "candidate", f"no archived candidate for {path.name}")
        graph, lock, _ = artifacts(path, original, framework, record)
        require(sha(graph) == record["graphSha256"], f"replay graph differs for {path.name}")
        require(semantic_lock_sha256(read(lock)) == record["semanticLockSha256"],
                f"replay semantic lock differs for {path.name}")
        print(path.name, record["graphSha256"], flush=True)


def recover(original, framework, work, triple, jobs, selected, feature_mode):
    checkout(framework, FRAMEWORK)
    _, all_cells = matrix(original)
    cells = [cell for cell in all_cells if cell["triple"] == triple
             and (not selected or cell["target"] in selected)
             and (feature_mode is None or cell["features"] == feature_mode)]
    require(cells and set(selected) <= {cell["target"] for cell in cells}, "unknown target selection")
    host = capture(original, "rustc", f"+{TOOLCHAIN}", "-vV")
    require(f"host: {triple}" in host.splitlines(), f"recovery requires a native {triple} host")
    env = {k: v for k, v in os.environ.items()
           if k not in ("RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
                        "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP")}
    require(jobs >= 1, "jobs must be positive")
    env["RUSTUP_TOOLCHAIN"] = TOOLCHAIN
    if triple.endswith("-linux-gnu"):
        sysroot = Path(capture(original, "rustc", f"+{TOOLCHAIN}", "--print", "sysroot"))
        for variable in ("LIBRARY_PATH", "LD_LIBRARY_PATH"):
            paths = (str(sysroot / "lib"), env.get(variable))
            env[variable] = os.pathsep.join(path for path in paths if path)
    work.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="shapes-frontend-") as target:
        subprocess.run(["cargo", f"+{TOOLCHAIN}", "build", "--locked", "--offline", "--bin",
                        "oddlang", "--target-dir", target], cwd=framework, env=env, check=True)
        cli = Path(target) / "debug/oddlang"

        def run(cell):
            name = key(cell)
            output = work / name
            if output.exists():
                artifacts(output, original, framework, cell)
                print(f"{name} (existing)", flush=True)
                return
            profile = work / f"{name}.toml"
            profile_pin = {k: v for k, v in pin(original, cell).items()
                           if k not in ("manifestSha256", "packageManifestSha256")}
            profile_pin.update(adapter=str(framework / ADAPTER / "run.sh"),
                               adapterInputs=INPUTS, sourceRevision=UPSTREAM)
            profile.write_text("".join(f"{k} = {json.dumps(v)}\n" for k, v in profile_pin.items()),
                               encoding="utf-8")
            log_path = work / f"{name}.log"
            with log_path.open("w") as log:
                result = subprocess.run([cli, "recover", "rust-shapes", original / "Cargo.toml",
                                         "--profile", profile, "--out", output],
                                        cwd=original, env=env, stdout=log, stderr=log)
            require(result.returncode == 0, f"{name} recovery failed, see {log_path}")
            artifacts(output, original, framework, cell)
            print(name, flush=True)

        errors = []
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            futures = [pool.submit(run, cell) for cell in cells]
            for future in as_completed(futures):
                try:
                    future.result()
                except (ValueError, OSError) as error:
                    errors.append(str(error))
        require(not errors, "\n".join(sorted(errors)))
    checkout(original, UPSTREAM)
    checkout(framework, FRAMEWORK)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("framework", type=Path)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("verify")
    check.add_argument("--archive", type=Path, default=ARCHIVE)
    seal_parser = sub.add_parser("seal")
    seal_parser.add_argument("archive", type=Path)
    seal_parser.add_argument("work", type=Path, nargs="+")
    comparison = sub.add_parser("compare")
    comparison.add_argument("work", type=Path)
    comparison.add_argument("--archive", type=Path, default=ARCHIVE)
    run = sub.add_parser("recover")
    run.add_argument("work", type=Path)
    run.add_argument("triple", choices=TRIPLES)
    run.add_argument("--jobs", type=int, default=1)
    run.add_argument("--target", action="append", default=[])
    run.add_argument("--feature-mode", choices=("default", *FEATURES))
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    if args.command == "verify":
        verify(original, framework, args.archive)
    elif args.command == "compare":
        compare(original, framework, args.archive, args.work)
    elif args.command == "seal":
        checkout(framework, FRAMEWORK)
        _, cells = matrix(original)
        known = {key(cell) for cell in cells}
        observed = {}
        for root in args.work:
            for path in root.iterdir():
                if path.is_dir():
                    require(path.name in known and path.name not in observed,
                            f"unexpected or duplicated profile directory {path}")
                    observed[path.name] = path
        seal(original, framework, args.archive, observed)
        verify(original, framework, args.archive)
    else:
        feature_mode = args.feature_mode
        features = None if feature_mode is None else [] if feature_mode == "default" else [feature_mode]
        recover(original, framework, args.work.resolve(), args.triple, args.jobs, args.target,
                features)


if __name__ == "__main__":
    main()
