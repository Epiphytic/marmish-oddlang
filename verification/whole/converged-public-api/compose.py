"""Compose two independently qualified workspace changes with a shared base."""
import argparse
import json
from pathlib import Path

BASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered"
ERRORS = "workspace-v7-engine-public-error-identity"
CONSTANT = BASE + "-limit"
OUTPUT = "workspace-v7-converged-public-api"


def load(directory, name):
    return json.loads((directory / (name + ".json")).read_bytes())


def compose(directory):
    workspace = load(directory, BASE)
    members = workspace["members"].copy()
    for name, expected in ((ERRORS, {"marmish-engine", "marmish-native"}),
                           (CONSTANT, {"marmish-prototype"})):
        parent = load(directory, name)
        if parent.keys() != workspace.keys() or parent["members"].keys() != members.keys():
            raise ValueError(f"changed workspace shape in {name}")
        for key in workspace.keys() - {"members"}:
            if parent[key] != workspace[key]:
                raise ValueError(f"{name} changed the shared {key}")
        changed = {key for key, row in parent["members"].items() if row != workspace["members"][key]}
        if changed != expected:
            raise ValueError(f"unexpected member changes in {name}: {sorted(changed)}")
        for member in changed:
            members[member] = parent["members"][member]
    return dict(workspace, members=members)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    encoded = (json.dumps(compose(args.portable), indent=2) + "\n").encode()
    output = args.portable / (OUTPUT + ".json")
    if args.check:
        if output.read_bytes() != encoded:
            parser.exit(1, "stale combined workspace manifest\n")
    else:
        output.write_bytes(encoded)
