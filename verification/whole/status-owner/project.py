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
INPUTS = ROOT / "oddlang/portable/native-status-owner-v1"
REFERENCE = "af54339d250290490d62d4475a77539b7e739071"
PINS = {
    "Cargo.lock": "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    "crates/marmish-native/src/daemon.rs": "cf2e7cc2a9e9a5a55db726286a53e595d607cc22288cdf7b7fed0fe6c2f439c8",
    "crates/marmish-native/src/ipc.rs": "ffeeba6b2d0b8fef1d44bea311e5548e8b7ac520d4b7d04286077547273b8a4a",
    "crates/marmish-native/src/identity.rs": "0f8d1e4bcc9865ada76cc757b8bddcbc3a15397f0075c57d0eb6bd967b2a349d",
    "crates/marmish-native/src/command_lifecycle.rs": "35b2114ae59fa078ce630f36c14ed7cf494d5dde2b3d594e673964bc34ca6f3c",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def registry(data):
    return {(package["name"], package["version"], package["source"], package["checksum"])
            for package in tomllib.loads(data.decode())["package"]
            if package.get("source", "").startswith("registry+")}


def verify_reference(original):
    head = subprocess.check_output(["git", "-C", original, "rev-parse", "HEAD"], text=True).strip()
    if head != REFERENCE:
        raise ValueError(f"expected original {REFERENCE}, got {head}")
    pinned = {}
    for path, expected in PINS.items():
        data = subprocess.check_output(["git", "-C", original, "show", f"{REFERENCE}:{path}"])
        if digest(data) != expected:
            raise ValueError(f"reference source pin differs for {path}")
        pinned[path] = data
    locked = registry((INPUTS / "owner-Cargo.lock").read_bytes())
    if not locked or not locked <= registry(pinned["Cargo.lock"]):
        raise ValueError("status owner dependencies differ from the reference lock")


def stage(root):
    manifest = json.loads((INPUTS / "package.json").read_bytes())
    files = [HERE / "project.py", INPUTS / "package.json", INPUTS / "package.lock"]
    pins = [manifest["cargoLock"]]
    pins.extend(component[key] for component in manifest["components"] for key in ("graph", "overlay"))
    for pin in pins:
        source = (INPUTS / pin["path"]).resolve()
        if not source.is_relative_to(INPUTS) or digest(source.read_bytes()) != pin["sha256"]:
            raise ValueError(f"status owner package input differs for {pin['path']}")
        files.append(source)
    for source in sorted(set(files)):
        output = root / source.relative_to(ROOT)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, output)


def generate(oddlang, output):
    subprocess.run([oddlang, "build", "native-package", "rust", INPUTS / "package.json", output], check=True)
    for generated, expected in (("Cargo.lock", "owner-Cargo.lock"), ("package.lock", "package.lock")):
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
        verify_reference(args.directory)
    else:
        generate(args.oddlang, args.directory)


if __name__ == "__main__":
    main()
