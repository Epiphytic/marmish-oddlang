"""Add only the recovered constant to the locked seven-member workspace."""
import argparse
import hashlib
import json
from pathlib import Path

BASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"
WORKSPACE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit.json"
PREVIOUS_PACKAGE = "prototype-buffer-package.json"
PACKAGE = "prototype-buffer-limit-package.json"
MEMBER = "marmish-prototype"
CONTRACT = "prototype-constant-v1"


def pin(root, name):
    return {"path": name, "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}


def compose(root, check=False):
    def output(name, data):
        path = root / name
        encoded = (json.dumps(data, indent=2) + "\n").encode()
        if check:
            if path.read_bytes() != encoded:
                raise ValueError(f"unexpected contract inputs in {path}")
        else:
            path.write_bytes(encoded)

    package = json.loads((root / PREVIOUS_PACKAGE).read_bytes())
    package["components"].append({
        "kind": "shared-declarations",
        "module": "limits",
        "reexports": ["MAX_LINE"],
        "graph": pin(root, CONTRACT + "/graph.json"),
        "overlay": pin(root, CONTRACT + "/rust.json"),
    })
    output(PACKAGE, package)
    workspace = json.loads((root / BASE).read_bytes())
    workspace["members"][MEMBER]["manifest"] = pin(root, PACKAGE)
    output(WORKSPACE, workspace)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    arguments = parser.parse_args()
    compose(arguments.portable, arguments.check)
