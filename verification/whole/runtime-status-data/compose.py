"""Extend the canonical record component with the recovered status record."""
import argparse
import hashlib
import json
from pathlib import Path

BASE = "workspace-v7-canonical-offline"
OUTPUT = "workspace-v7-canonical-runtime-status"
DOMAIN = "native-journal-runtime-status-v1"
PACKAGE = "marmish-native-runtime-status-data-package.json"
MEMBER = "marmish-native"


def pin(root, path):
    return {"path": path, "sha256": hashlib.sha256((root / path).read_bytes()).hexdigest()}


def write(root, path, value, check):
    output = (json.dumps(value, indent=2) + "\n").encode()
    path = root / path
    if check:
        if path.read_bytes() != output:
            raise ValueError(f"composed input differs: {path}")
    else:
        path.write_bytes(output)


def compose(root, check=False):
    spec = json.loads((root / (BASE + ".json")).read_bytes())
    member = spec["members"][MEMBER]
    prior = member["manifest"]
    if pin(root, prior["path"]) != prior:
        raise ValueError("canonical native manifest digest mismatch")
    package = json.loads((root / prior["path"]).read_bytes())
    component, = (row for row in package["components"] if row["module"] == "record_views")
    if component["kind"] != "public-records":
        raise ValueError("canonical record view kind differs")
    component.update(graph=pin(root, f"{DOMAIN}/graph.json"), overlay=pin(root, f"{DOMAIN}/rust.json"))
    write(root, PACKAGE, package, check)
    member["manifest"] = pin(root, PACKAGE)
    write(root, OUTPUT + ".json", spec, check)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("portable", type=Path)
    p.add_argument("--check", action="store_true")
    args = p.parse_args()
    compose(args.portable, args.check)
