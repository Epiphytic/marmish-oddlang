#!/usr/bin/env python3
"""Recover the pinned native parser or build the two graph-only executables."""

import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
INPUTS = ROOT / "oddlang/portable/native-cli-v1"
REVISION = "af54339d250290490d62d4475a77539b7e739071"
PARSER = "crates/marmish-native/src/cli.rs"
MANIFEST = "crates/marmish-native/Cargo.toml"
PINS = {
    "Cargo.lock": "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    PARSER: "a4122418f4f777a53e839487b3b162951ac2b1447ea26b968245322390c866d4",
    MANIFEST: "f92cdde82b3e4cf077294a7296117af4dcc28dba55bf1c9d694d7a4b273f539f",
}


def recover(oddlang, original):
    head = subprocess.check_output(["git", "-C", original, "rev-parse", "HEAD"], text=True).strip()
    if head != REVISION:
        raise ValueError(f"expected original {REVISION}, got {head}")
    pinned = {}
    for path, expected in PINS.items():
        contents = subprocess.check_output(["git", "-C", original, "show", f"{REVISION}:{path}"])
        if hashlib.sha256(contents).hexdigest() != expected:
            raise ValueError(f"source pin differs for {path}")
        pinned[path] = contents
    package = tomllib.loads(pinned[MANIFEST].decode())["package"]
    if (package["name"], package["version"], package["edition"]) != ("marmish-native", "0.3.1", "2024"):
        raise ValueError("unexpected original package metadata")
    def registry_packages(lock):
        return {
            (record["name"], record["version"], record["source"], record["checksum"])
            for record in tomllib.loads(lock.decode())["package"]
            if record.get("source", "").startswith("registry+")
        }

    required = registry_packages((INPUTS / "parser-Cargo.lock").read_bytes())
    available = registry_packages(pinned["Cargo.lock"])
    if not required or not required.issubset(available):
        raise ValueError("generated parser dependency pins differ from the original release")
    with tempfile.TemporaryDirectory(prefix="native-cli-recovery-") as temp:
        scratch = Path(temp)
        source = scratch / "parser.rs"
        graph = scratch / "candidate.json"
        source.write_bytes(pinned[PARSER])
        subprocess.run(
            [oddlang, "recover", "cli-surface", source, INPUTS / "selection.json", graph],
            check=True,
        )
        if graph.read_bytes() != (INPUTS / "graph.json").read_bytes():
            raise ValueError("recovered graph differs from published candidate")


def generate(oddlang, output):
    subprocess.run(
        [oddlang, "build", "native-workspace", "rust", INPUTS / "workspace.json", output],
        check=True,
    )
    for generated in (Path("workspace.lock"), Path("marmish-native/package.lock")):
        if (output / generated).read_bytes() != (INPUTS / generated.name).read_bytes():
            raise ValueError(f"generated {generated} differs from the reviewed lock")
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "--offline", "--bins"],
        cwd=output,
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["recover", "generate"])
    parser.add_argument("oddlang", type=lambda path: Path(path).resolve())
    parser.add_argument("directory", type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    try:
        if args.mode == "recover":
            recover(args.oddlang, args.directory)
        else:
            generate(args.oddlang, args.directory)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
