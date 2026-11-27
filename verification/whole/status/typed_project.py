#!/usr/bin/env python3
"""Stage the graph-only typed status package for isolated native generation."""

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
SHAPE = PORTABLE / "native-status-typed-v1"
PACKAGE = PORTABLE / "native-status-typed-package.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pinned(pin):
    path = (PORTABLE / pin["path"]).resolve()
    if not path.is_relative_to(PORTABLE) or digest(path) != pin["sha256"]:
        raise ValueError(f"typed status pin differs for {pin['path']}")
    return path


def stage(destination):
    package = json.loads(PACKAGE.read_bytes())
    files = {PACKAGE, Path(__file__), SHAPE / "package.lock", pinned(package["cargoLock"])}
    for component in package["components"]:
        files.update(pinned(component[key]) for key in ("graph", "overlay"))
    for path in files:
        copied = destination / path.relative_to(ROOT)
        copied.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, copied)


def recover(oddlang, original):
    subprocess.run(
        ["cargo", "+1.96.1", "run", "--quiet", "--offline", "--locked",
         "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
         "--bin", "status-project", "--", "recover", oddlang, original], check=True,
    )
    provenance = json.loads((SHAPE / "provenance.json").read_bytes())

    def registry(raw):
        return {(dep["name"], dep["version"], dep["source"], dep["checksum"])
                for dep in tomllib.loads(raw.decode())["package"] if "checksum" in dep}

    revision = provenance["originalRevision"]
    for path, expected in provenance["sourceSha256"].items():
        source = subprocess.check_output(["git", "-C", original, "show", f"{revision}:{path}"])
        if hashlib.sha256(source).hexdigest() != expected:
            raise ValueError(f"typed reply reference pin differs for {path}")
    original_lock = subprocess.check_output(["git", "-C", original, "show", f"{revision}:Cargo.lock"])
    actual_lock = pinned(json.loads(PACKAGE.read_bytes())["cargoLock"])
    if not registry(actual_lock.read_bytes()) <= registry(original_lock):
        raise ValueError("typed status registry dependencies differ from the original lock")


def generate(oddlang, output):
    subprocess.run([oddlang, "build", "native-package", "rust", PACKAGE, output], check=True)
    if (output / "package.lock").read_bytes() != (SHAPE / "package.lock").read_bytes():
        raise ValueError("generated typed status package lock differs")
    subprocess.run(["cargo", "build", "--release", "--locked", "--offline", "--bins"], cwd=output, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    staging = sub.add_parser("stage")
    staging.add_argument("directory", type=lambda value: Path(value).resolve())
    for mode in ("recover", "generate"):
        command = sub.add_parser(mode)
        command.add_argument("oddlang", type=lambda value: Path(value).resolve())
        command.add_argument("directory", type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    if args.command == "stage":
        stage(args.directory)
    elif args.command == "recover":
        recover(args.oddlang, args.directory)
    else:
        generate(args.oddlang, args.directory)


if __name__ == "__main__":
    main()
