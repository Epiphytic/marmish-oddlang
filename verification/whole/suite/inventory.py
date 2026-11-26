#!/usr/bin/env python3
"""Enumerate tracked evidence entry points from the pinned upstream Git tree."""

import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess

PIN = "af54339d250290490d62d4475a77539b7e739071"
BIN_ENV = re.compile(rb'(?:option_)?env!\(\s*"CARGO_BIN_EXE_([^"]+)"')
RUST_TEST = re.compile(rb'^\s*#\[\s*(?:(?:tokio|async_std)::)?test\b', re.M)


def git(upstream, *args):
    return subprocess.check_output(["git", "-C", str(upstream), *args])


def inventory(upstream):
    if git(upstream, "rev-parse", "HEAD").decode().strip() != PIN:
        raise ValueError(f"expected upstream HEAD {PIN}")

    result = {
        "upstream_commit": PIN,
        "upstream_tree": git(upstream, "rev-parse", f"{PIN}^{{tree}}").decode().strip(),
        "rust_unit_source_files": [],
        "rust_integration_targets": [],
        "rust_integration_support_files": [],
        "python_test_modules": [],
        "node_test_modules": [],
        "c_fixture_test_sources": [],
        "workflow_files": [],
    }
    for entry in git(upstream, "ls-tree", "-r", "-z", PIN).split(b"\0"):
        if not entry:
            continue
        header, name = entry.split(b"\t", 1)
        path = PurePosixPath(name.decode())
        row = {"path": str(path), "git_blob": header.split()[2].decode()}
        if path.parts[:2] == (".github", "workflows"):
            key = "workflow_files"
        elif path.name.endswith(".test.mjs"):
            key = "node_test_modules"
        elif path.suffix == ".py" and path.name.startswith("test_"):
            key = "python_test_modules"
        elif path.name.endswith("-test.c"):
            key = "c_fixture_test_sources"
        elif path.suffix == ".rs" and path.parts[0] in ("crates", "tools"):
            source = git(upstream, "cat-file", "blob", row["git_blob"])
            references = sorted({s.decode() for s in BIN_ENV.findall(source)})
            if references:
                row["compile_time_executables"] = references
            if path.parts[2:3] == ("tests",):
                key = ("rust_integration_targets" if len(path.parts) == 4
                       else "rust_integration_support_files")
            elif RUST_TEST.search(source):
                key = "rust_unit_source_files"
            else:
                continue
        else:
            continue
        result[key].append(row)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    args = parser.parse_args()
    print(json.dumps(inventory(args.upstream), indent=2))


if __name__ == "__main__":
    main()
