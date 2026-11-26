#!/usr/bin/env python3
"""Verify original status-owner provenance and render the portable native package."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
INPUTS = PORTABLE / "native-foreground-owner-v1"
MANIFEST = PORTABLE / "native-foreground-package.json"
REFERENCE = "af54339d250290490d62d4475a77539b7e739071"
CARGO_LOCK = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def registry(data):
    return {(package["name"], package["version"], package["source"], package["checksum"])
            for package in tomllib.loads(data.decode())["package"]
            if package.get("source", "").startswith("registry+")}


def verify_reference(oddlang, original):
    for sibling in ("cli", "status-owner", "status"):
        subprocess.run(
            ["python3", HERE.parent / sibling / "project.py", "recover", oddlang, original],
            check=True,
        )
    profile = json.loads((INPUTS / "graph.json").read_bytes())
    reader = json.loads((PORTABLE / "native-status-discovery-v1/graph.json").read_bytes())["discovery"]
    shared = profile["profile"]
    runtime = profile["runtime"]
    checks = (
        (profile["arguments"]["state"], reader["state_argument"]),
        (profile["arguments"]["identity"], reader["profile_argument"]),
        (shared["document"], reader["profile"]["leaf"]),
        (shared["maximum_bytes"], reader["profile"]["maximum_bytes"]),
        (shared["name_limit"], reader["profile_atom"]["maximum_bytes"]),
        (set(shared["fields"].values()), {field["key"] for field in reader["profile"]["fields"]}),
        (runtime["binding"], reader["binding"]["leaf"]),
        (set(runtime["fields"].values()), {field["key"] for field in reader["binding"]["fields"]}),
        (runtime["prefix"] + "-", reader["runtime"][0]["value"]),
    )
    if any(left != right for left, right in checks):
        raise ValueError("foreground publication and independent endpoint discovery layouts differ")
    locked = subprocess.check_output(["git", "-C", original, "show", f"{REFERENCE}:Cargo.lock"])
    native = registry((INPUTS / "foreground-Cargo.lock").read_bytes())
    if digest(locked) != CARGO_LOCK or not native or not native <= registry(locked):
        raise ValueError("foreground owner dependency closure differs from the reference lock")


def stage(root):
    manifest = json.loads(MANIFEST.read_bytes())
    files = [HERE / "project.py", MANIFEST, INPUTS / "package.lock"]
    pins = [manifest["cargoLock"]]
    pins.extend(component[key] for component in manifest["components"] for key in ("graph", "overlay"))
    for pin in pins:
        source = (PORTABLE / pin["path"]).resolve()
        if not source.is_relative_to(PORTABLE) or digest(source.read_bytes()) != pin["sha256"]:
            raise ValueError(f"status owner package input differs for {pin['path']}")
        files.append(source)
    for source in sorted(set(files)):
        output = root / source.relative_to(ROOT)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, output)


def generate(oddlang, output):
    subprocess.run([oddlang, "build", "native-package", "rust", MANIFEST, output], check=True)
    for generated, expected in (("Cargo.lock", "foreground-Cargo.lock"), ("package.lock", "package.lock")):
        if (output / generated).read_bytes() != (INPUTS / expected).read_bytes():
            raise ValueError(f"generated {generated} differs from the reviewed input")
    subprocess.run(["cargo", "build", "--release", "--locked", "--offline"], cwd=output, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    path = lambda value: Path(value).resolve()
    sub.add_parser("stage").add_argument("directory", type=path)
    for mode in ("recover", "generate"):
        command = sub.add_parser(mode)
        command.add_argument("oddlang", type=path)
        command.add_argument("directory", type=path)
    args = parser.parse_args()
    if args.mode == "stage":
        stage(args.directory)
    elif args.mode == "recover":
        verify_reference(args.oddlang, args.directory)
    else:
        generate(args.oddlang, args.directory)


if __name__ == "__main__":
    main()
