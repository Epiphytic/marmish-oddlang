#!/usr/bin/env python3
"""Build the checked native status discovery inputs in the generation cell."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
POLICY = PORTABLE / "native-status-discovery-v1"
PACKAGE = PORTABLE / "native-status-discovery-package.json"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pinned(pin):
    path = (PORTABLE / pin["path"]).resolve()
    if not path.is_relative_to(PORTABLE) or sha(path.read_bytes()) != pin["sha256"]:
        raise ValueError(f"status discovery input differs: {pin['path']}")
    return path


def stage(destination):
    package = json.loads(PACKAGE.read_bytes())
    files = {PACKAGE, Path(__file__), POLICY / "package.lock", POLICY / "endpoint-obligations.json",
             pinned(package["cargoLock"])}
    for component in package["components"]:
        files.update(pinned(component[key]) for key in ("graph", "overlay"))
    for path in files:
        copied = destination / path.relative_to(ROOT)
        copied.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, copied)


def recover(oddlang, original):
    from typed_project import recover as verify_reference

    verify_reference(oddlang, original)
    provenance = json.loads((POLICY / "provenance.json").read_bytes())
    revision = provenance["originalRevision"]
    for path, expected in provenance["sourceSha256"].items():
        source = subprocess.check_output(["git", "-C", original, "show", f"{revision}:{path}"])
        if sha(source) != expected:
            raise ValueError(f"endpoint reference pin differs for {path}")


def generate(oddlang, output):
    with tempfile.TemporaryDirectory(dir=output.parent) as directory:
        projection = Path(directory) / "projection"
        subprocess.run([oddlang, "build", "local-command-client", "rust", POLICY / "graph.json",
                        PORTABLE / "native-status-v1/rust.json", projection], check=True)
        if (projection / "endpoint-obligations.json").read_bytes() != (POLICY / "endpoint-obligations.json").read_bytes():
            raise ValueError("endpoint obligations differ")
    subprocess.run([oddlang, "build", "native-package", "rust", PACKAGE, output], check=True)
    if (output / "package.lock").read_bytes() != (POLICY / "package.lock").read_bytes():
        raise ValueError("status discovery package lock differs")
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
