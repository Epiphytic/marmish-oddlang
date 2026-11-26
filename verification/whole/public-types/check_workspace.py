"""Verify the archived public type evidence covers each pinned workspace library."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"


def read(path):
    return json.loads(path.read_bytes())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_sources(upstream, generated, pins, release, release_pin):
    git = ["git", "-C", upstream]
    require(
        subprocess.check_output([*git, "rev-parse", "HEAD"], text=True).strip()
        == pins["original"]["revision"],
        "original revision mismatch",
    )
    require(not subprocess.check_output([*git, "status", "--porcelain"]), "dirty upstream checkout")
    entries = tomllib.loads((upstream / "Cargo.toml").read_text())["workspace"]["members"]
    original = {}
    for entry in entries:
        manifest = upstream / entry / "Cargo.toml"
        package = tomllib.loads(manifest.read_text())
        require(
            (manifest.parent / package.get("lib", {}).get("path", "src/lib.rs")).is_file(),
            f"missing original library in {entry}",
        )
        original[package["package"]["name"]] = manifest
    lock_path = PORTABLE / f"workspace-{release}/workspace.lock"
    require(
        sha(lock_path) == release_pin["workspaceLockSha256"]
        and lock_path.read_bytes() == (generated / "workspace.lock").read_bytes(),
        "wrong generated workspace lock",
    )
    lock = read(lock_path)
    manifest = PORTABLE / f"workspace-{release}.json"
    require(
        sha(manifest) == lock["inputSha256"]
        and read(manifest)["members"].keys() == lock["members"].keys(),
        "release manifest differs from workspace lock",
    )
    require(release_pin["members"].keys() == lock["members"].keys(), "provenance member set differs")
    require(
        sha(generated / "Cargo.toml") == lock["workspaceCargoManifestSha256"]
        and sha(generated / "Cargo.lock") == lock["cargoLockSha256"],
        "workspace inputs differ",
    )
    candidates = {}
    for name, member in lock["members"].items():
        pkg = generated / name
        require(
            sha(pkg / "Cargo.toml") == member["workspaceCargoManifestSha256"]
            and sha(pkg / "package.lock") == member["standalonePackageLockSha256"],
            f"wrong member {name}",
        )
        expected = read(pkg / "package.lock")["generatedFiles"]
        actual = {str(p.relative_to(pkg)): sha(p) for p in (pkg / "src").rglob("*") if p.is_file()}
        require(
            actual == expected and not (pkg / "build.rs").exists(),
            f"changed {release} member sources in {name}",
        )
        require(
            member["standalonePackageLockSha256"] == release_pin["members"][name],
            f"wrong pinned package lock {name}",
        )
        candidates[name] = pkg / "Cargo.toml"
    return original, candidates


def caller_counts(file, expected_path, subject_path, package):
    receipt = read(file)
    subject_lock = read(subject_path.with_name("exports.lock"))["pin"]
    require(
        receipt["expectedExportsSha256"] == sha(expected_path)
        and receipt["subjectExportsSha256"] == sha(subject_path)
        and receipt["subjectLockSha256"] == sha(subject_path.with_name("exports.lock"))
        and receipt["compiler"] == subject_lock["compilerCommit"]
        and receipt["subjectTargetName"] == subject_lock["targetName"]
        and receipt["subjectPackage"] == package
        and receipt["target"] == "aarch64-apple-darwin",
        "unbound caller evidence",
    )

    def kinds(path):
        return {
            tuple(row["path"]) + (name,): edge["kind"]
            for row in read(path)["modules"].values()
            for name, edge in row["bindings"].items()
        }

    wanted, subject = kinds(expected_path), kinds(subject_path)
    rows = receipt["imports"]
    require(
        [tuple(row["spelling"]) for row in rows] == sorted(wanted), "missing or repeated import probes"
    )
    for row in rows:
        key = tuple(row["spelling"])
        require(
            wanted[key] == row["expectedKind"] and subject.get(key) == row["subjectKind"],
            "wrong namespace classification",
        )
        if key in subject:
            require(row["importCompiles"] and not row["errorCodes"], "public import rejected")
        else:
            require(
                not row["importCompiles"] and row["errorCodes"] == ["E0432"],
                "difference from the observed unresolved import",
            )
    passed = sum(r["importCompiles"] for r in rows)
    return {"compiled": passed, "rejected": len(rows) - passed}


def summarize(upstream, generated, release, extractor):
    pins = read(HERE / "provenance.json")
    release_pin = pins["generated"] if release == "v5" else pins["generatedV6"]
    originals, candidates = validate_sources(upstream, generated, pins, release, release_pin)
    profiles = {k: v for k, v in pins["profiles"].items() if k.startswith("original-")}
    profiles.update(
        {k: v for k, v in pins["profiles"].items() if k.startswith("generated-")}
        if release == "v5"
        else pins["profilesV6"]
    )
    roles = ("original", "generated" if release == "v5" else "generated-v6")
    rows = []
    observed = set()
    for name, manifest in sorted(originals.items()):
        row = dict(package=name)
        suffix = name.removeprefix("marmish-")
        for role, path in zip(roles, (manifest, candidates.get(name)), strict=True):
            if path is None:
                row["original" if role == "original" else "generated"] = None
                continue
            folder = f"{role}-{suffix}"
            observed.add(folder)
            spec = profiles[folder]
            dest = HERE / folder
            for field, file in (
                ("exportsSha256", "exports.json"),
                ("lockSha256", "exports.lock"),
                ("evidenceSha256", "evidence.json"),
            ):
                require(sha(dest / file) == spec[field], f"{folder}/{file} digest mismatch")
            lock, e = read(dest / "exports.lock"), read(dest / "evidence.json")
            pin = lock["pin"]
            workspace = upstream if role == "original" else generated
            require(
                pin["package"] == name == e["package"]
                and pin["targetKind"] == "lib"
                and pin["packageManifestSha256"] == sha(path)
                and pin["manifestSha256"] in {sha(workspace / "Cargo.toml"), sha(path)}
                and pin["cargoLockSha256"] == sha(workspace / "Cargo.lock")
                and lock["exportsSha256"] == e["exportsSha256"] == spec["exportsSha256"]
                and e["deterministic"]
                and e["defaultFeatures"]
                and not e["features"]
                and pin["defaultFeatures"]
                and not pin["features"]
                and pin["targetTriple"] == e["target"] == "aarch64-apple-darwin"
                and pin["compilerCommit"] == e["compiler"],
                f"wrong profile or input {folder}",
            )
            access = e["fieldProbes"]
            row["original" if role == "original" else "generated"] = dict(
                profile=folder,
                callerSpellings=e["compilePositiveCount"],
                rustdocSpellings=e["rustdocSpellingCount"],
                discrepancies=len(e["discrepancies"]),
                publicReads=access["publicReads"],
                restrictedRejections=access["privateRejections"],
                fieldProbeGaps=len(access["gaps"]),
            )
        if name in candidates:
            comparison = HERE / ((suffix if release == "v5" else suffix + "-v6") + "-comparison.json")
            original_graph = HERE / ("original-" + suffix) / "exports.json"
            candidate_graph = HERE / (roles[1] + "-" + suffix) / "exports.json"
            with tempfile.TemporaryDirectory() as directory:
                temp = Path(directory)
                for tool in ("probe_exports.py", "compare_exports.py"):
                    code = subprocess.check_output(
                        [
                            "git",
                            "-C",
                            extractor,
                            "show",
                            pins["extractorRevision"] + ":verification/rust-recovery/" + tool,
                        ]
                    )
                    (temp / tool).write_bytes(code)
                output = temp / "diff.json"
                subprocess.run(
                    [
                        sys.executable,
                        "-B",
                        temp / "compare_exports.py",
                        original_graph,
                        candidate_graph,
                        output,
                    ],
                    capture_output=True,
                    check=True,
                )
                require(output.read_bytes() == comparison.read_bytes(), f"comparison drift in {name}")
            c = read(comparison)
            require(
                c["originalSha256"] == profiles["original-" + suffix]["exportsSha256"]
                and c["generatedSha256"] == profiles[roles[1] + "-" + suffix]["exportsSha256"],
                "comparison input mismatch",
            )
            row["comparison"] = {
                key: len(c[key]) for key in ("originalOnly", "generatedOnly", "changedAtSharedSpelling")
            }
            row["comparison"]["shared"] = c["sharedCount"]
            row["originalImportsAgainstGenerated"] = caller_counts(
                HERE / f"{suffix}-callers-{release}.json", original_graph, candidate_graph, name
            )
            if name == "marmish-native" and release == "v6":
                row["generatedImportsAgainstOriginal"] = caller_counts(
                    HERE / "native-callers-rev.json", candidate_graph, original_graph, name
                )
        rows.append(row)
    require(
        set(candidates) <= set(originals) and observed == profiles.keys(),
        "unexpected or missing package",
    )
    return {
        "release": release,
        "target": "aarch64-apple-darwin",
        "profile": "default release",
        "packages": rows,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("generated", type=Path)
    parser.add_argument("extractor", type=Path)
    parser.add_argument("--release", required=True, choices=("v5", "v6"))
    args = parser.parse_args()
    summary = summarize(
        args.upstream.resolve(), args.generated.resolve(), args.release, args.extractor.resolve()
    )
    require(
        summary == read(HERE / ("workspace-" + args.release + ".json")),
        "workspace coverage summary drifted",
    )
    print(
        f"Verified {len(summary['packages'])} original libraries against the {args.release} member set."
    )
