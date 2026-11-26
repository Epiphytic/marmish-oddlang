#!/usr/bin/env python3
"""Stage and generate the native status graph workspace."""

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
STATUS = PORTABLE / "native-status-v1"
WORKSPACE = PORTABLE / "native-status-workspace.json"
ORIGINAL_REVISION = "af54339d250290490d62d4475a77539b7e739071"
ORIGINAL_PINS = {
    "Cargo.lock": "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    "crates/marmish-native/src/ipc.rs": "ffeeba6b2d0b8fef1d44bea311e5548e8b7ac520d4b7d04286077547273b8a4a",
    "crates/marmish-native/src/error.rs": "54eb8a335f91cb09de6ecd8d598acdec3476ee7067968333f85c824a5fee2254",
    "crates/marmish-native/src/transport.rs": "5490e5d39761df1fc386249c3e901a920c4343257836908a6f46746a22c06800",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def _pinned(base, pin):
    source = (base / pin["path"]).resolve()
    if not source.is_relative_to(base) or sha(source.read_bytes()) != pin["sha256"]:
        raise ValueError(f"invalid input pin {pin['path']}")
    return source


def stage(destination):
    manifest = json.loads(WORKSPACE.read_bytes())
    files = [WORKSPACE, HERE / "project.py", STATUS / "workspace.lock", STATUS / "package.lock"]
    files.append(_pinned(PORTABLE, manifest["cargoLock"]))
    for member in manifest["members"].values():
        member_path = _pinned(PORTABLE, member["manifest"])
        files.append(member_path)
        package = json.loads(member_path.read_bytes())
        files.append(_pinned(member_path.parent, package["cargoLock"]))
        for component in package["components"]:
            files.extend(_pinned(member_path.parent, component[key]) for key in ("graph", "overlay"))
    for source in sorted(set(files)):
        target = destination / source.relative_to(ROOT)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)


def verify_reference(oddlang, original):
    subprocess.run(
        ["python3", HERE.parent / "cli/project.py", "recover", oddlang, original], check=True,
    )
    pinned = {}
    for path, digest in ORIGINAL_PINS.items():
        pinned[path] = subprocess.check_output(["git", "-C", original, "show", f"{ORIGINAL_REVISION}:{path}"])
        if sha(pinned[path]) != digest:
            raise ValueError(f"reference source pin differs for {path}")

    def registry(lock):
        return {(p["name"], p["version"], p["source"], p["checksum"])
                for p in tomllib.loads(lock.decode())["package"]
                if p.get("source", "").startswith("registry+")}

    if not registry((STATUS / "client-Cargo.lock").read_bytes()) <= registry(pinned["Cargo.lock"]):
        raise ValueError("status dependencies differ from the reference source and checksums")


def generate(oddlang, output):
    subprocess.run([oddlang, "build", "native-workspace", "rust", WORKSPACE, output], check=True)
    for relative, expected in (
        ("Cargo.lock", "client-Cargo.lock"),
        ("workspace.lock", "workspace.lock"),
        ("marmish-native/package.lock", "package.lock"),
    ):
        if (output / relative).read_bytes() != (STATUS / expected).read_bytes():
            raise ValueError(f"generated {relative} differs from the reviewed lock")
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
        verify_reference(args.oddlang, args.directory)
    else:
        generate(args.oddlang, args.directory)


if __name__ == "__main__":
    main()
