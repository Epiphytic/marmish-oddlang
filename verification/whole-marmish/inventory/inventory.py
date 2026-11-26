"""Produce a portable baseline inventory from an immutable Cargo workspace snapshot."""
import argparse
import fnmatch
import hashlib
import json
import subprocess
import tempfile
import tomllib
from pathlib import Path


def run(root, *command):
    return subprocess.check_output(command, cwd=root)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cargo_workspace(root, manifest):
    data = json.loads(run(root, "cargo", "metadata", "--offline", "--locked", "--no-deps",
                          "--format-version", "1", "--manifest-path", manifest))
    packages = []
    for item in sorted(data["packages"], key=lambda p: p["name"]):
        targets = []
        for target in sorted(item["targets"], key=lambda t: (t["kind"], t["name"])):
            targets.append({
                "name": target["name"], "kind": target["kind"], "crate_types": target["crate_types"],
                "path": Path(target["src_path"]).relative_to(root).as_posix(),
                "required_features": sorted(target.get("required-features", [])),
                "test": target["test"], "doctest": target["doctest"], "edition": target["edition"]
            })
        packages.append({
            "name": item["name"], "version": item["version"], "edition": item["edition"],
            "minimum_rust": item["rust_version"], "features": item["features"],
            "manifest": Path(item["manifest_path"]).relative_to(root).as_posix(), "targets": targets
        })
    lock = (root / manifest).with_name("Cargo.lock")
    return {"manifest": manifest, "lock_sha256": sha256(lock), "packages": packages}


def inspect(root, spec):
    head = run(root, "git", "rev-parse", "HEAD").decode().strip()
    if head != spec["commit"]:
        raise ValueError("revision differs from the release pin")
    workspaces = [cargo_workspace(root, name) for name in spec["workspaces"]]
    files = run(root, "git", "ls-files", "-z").decode().split("\0")[:-1]
    tracked = set(files)
    groups = {rule["id"]: [] for rule in spec["surfaces"]}
    for path in files:
        if path.split("/", 1)[0] in spec["inventory_roots"]:
            for rule in spec["surfaces"]:
                if any(fnmatch.fnmatchcase(path, pattern) for pattern in rule["paths"]):
                    groups[rule["id"]].append(path)
                    break
            else:
                raise ValueError(f"unclassified input {path}")
    release = spec["release"]
    for path in release["witnesses"] + [p["witness"] for p in release["external_programs"]]:
        if path not in tracked:
            raise ValueError(f"missing release witness {path}")
    covered_manifests = set(spec["workspaces"])
    covered_manifests.update(p["manifest"] for w in workspaces for p in w["packages"])
    actual_manifests = {path for path in files if Path(path).name == "Cargo.toml"}
    if actual_manifests != covered_manifests:
        raise ValueError(f"unclassified Cargo manifests: {actual_manifests ^ covered_manifests}")
    packages = {p["name"]: p for workspace in workspaces for p in workspace["packages"]}
    matrix = []
    for profile, features in sorted(release["profiles"].items()):
        for feature in features:
            package, name = feature.split("/")
            if name not in packages[package]["features"]:
                raise ValueError(f"unknown feature {feature}")
        for target in sorted(release["platforms"]):
            matrix.append({"profile": profile, "target": target, "cargo_features": features})
    version = packages[release["package"]]["version"]
    return {
        "source": spec["source"], "commit": head,
        "previous_commit": spec["previous_commit"],
        "changed_paths": run(root, "git", "diff", "--name-only", "-z",
                             spec["previous_commit"], head).decode().split("\0")[:-1],
        "tree": run(root, "git", "rev-parse", "HEAD^{tree}").decode().strip(),
        "toolchain": tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"],
        "workspaces": workspaces, "file_groups": groups,
        "platform_contracts": release["platforms"], "baseline_cells": matrix,
        "release_assets": sorted(template.format(version=version, target=target)
                                 for target in release["platforms"] for template in release["payloads"])
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo", type=Path)
    parser.add_argument("spec", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    spec = json.loads(args.spec.read_bytes())
    with tempfile.TemporaryDirectory() as stage:
        root = Path(stage).resolve() / "source"
        run(stage, "git", "clone", "--quiet", "--no-checkout", "--no-local", str(args.repo.resolve()), str(root))
        run(root, "git", "checkout", "--quiet", "--detach", spec["commit"])
        result = inspect(root, spec)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
