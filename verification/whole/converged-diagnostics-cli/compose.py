"""Compose sealed workspace branches by native component identity."""
import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
BASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered"
PUBLIC = "workspace-v7-converged-public-api"
CLI = "workspace-v7-offline-cli"
DIAGNOSTICS = "workspace-v7-offline-cli-diagnostics"
OUTPUT = "workspace-v7-converged-diagnostics-cli"


def load(path):
    return json.loads(path.read_bytes())


def encode(record):
    return (json.dumps(record, indent=2, ensure_ascii=False) + "\n").encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pin(root, path, contents):
    return {"path": path.relative_to(root).as_posix(), "sha256": sha(contents)}


def checked_package(root, descriptor):
    path = (root / descriptor["path"]).resolve()
    if path.parent != root or not path.is_file():
        raise ValueError(f"merged packages must be siblings of the workspace manifest: {path}")
    contents = path.read_bytes()
    if descriptor != pin(root, path, contents):
        raise ValueError(f"package pin differs: {path}")
    return json.loads(contents)


def components(manifest):
    rows = {}
    for component in manifest["components"]:
        key = component["kind"], component["module"]
        if key in rows:
            raise ValueError(f"duplicate native component {key!r}")
        rows[key] = component
    return rows


def combine_packages(base, branches):
    """Preserve inherited order, merge disjoint edits and reject conflicting edits."""
    parent = components(base)
    previous = {key: value for key, value in base.items() if key != "components"}
    changes = {}
    for branch in branches:
        if {key: value for key, value in branch.items() if key != "components"} != previous:
            raise ValueError("native package metadata differs beyond component edits")
        edited = components(branch)
        if not parent.keys() <= edited.keys():
            raise ValueError("native package lost inherited components")
        for key, component in edited.items():
            if component == parent.get(key):
                continue
            if key in changes and changes[key] != component:
                raise ValueError(f"incompatible edits to native component {key!r}")
            changes[key] = component
    order = list(parent) + sorted(changes.keys() - parent.keys())
    return dict(base, components=[changes[key] if key in changes else parent[key] for key in order])


def compose(root, check):
    root = root.resolve()
    reference = load(root / f"{BASE}.json")
    branches = [load(root / f"{name}.json") for name in (PUBLIC, CLI, DIAGNOSTICS)]
    members = reference["members"].copy()
    for branch in branches:
        if (branch.keys() != reference.keys() or members.keys() != branch["members"].keys()
                or any(branch[key] != reference[key] for key in reference.keys() - {"members"})):
            raise ValueError("workspace membership or common dependency lock differs")
    files = {}
    for name, member in reference["members"].items():
        variants = [branch["members"][name] for branch in branches if branch["members"][name] != member]
        if not variants:
            continue
        if any({key: value for key, value in row.items() if key != "manifest"}
               != {key: value for key, value in member.items() if key != "manifest"} for row in variants):
            raise ValueError(f"changed dependency edges for {name}")
        base = checked_package(root, member["manifest"])
        donors = [checked_package(root, row["manifest"]) for row in variants]
        combined = combine_packages(base, donors)
        for row, donor in zip(variants, donors):
            if donor == combined:
                members[name] = row
                break
        else:
            output = root / f"{name}-converged-diagnostics-cli-package.json"
            content = encode(combined)
            files[output] = content
            members[name] = dict(member, manifest=pin(root, output, content))
    files[root / f"{OUTPUT}.json"] = encode(dict(reference, members=members))
    for path, content in files.items():
        if check:
            if path.read_bytes() != content:
                raise ValueError(f"sealed composition differs: {path}")
        else:
            path.write_bytes(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path, nargs="?", default=PORTABLE)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    compose(args.portable, args.check)
