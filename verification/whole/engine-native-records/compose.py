"""Add the scalar engine and native path record groups to the converged workspace."""
import argparse
import hashlib
import json
from pathlib import Path

BASE = "workspace-v7-converged-public-api.json"
WORKSPACE = "workspace-v7-observational-records.json"
GROUPS = {
    "engine": {
        "domain": "engine-scalar-records-v1", "module": "model",
        "types": ["InputReceipt", "MaintenanceReport"], "publicModule": False,
    },
    "native": {
        "domain": "native-path-records-v1", "module": "transport",
        "types": ["ArtiDirs"], "publicModule": True,
    },
}


def data(path):
    return json.loads(path.read_bytes())


def pin(root, name):
    return {"path": name, "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}


def write(root, name, value, check):
    encoded = (json.dumps(value, indent=2) + "\n").encode()
    target = root / name
    if check:
        if target.read_bytes() != encoded:
            raise ValueError(f"composed descriptor differs: {name}")
    else:
        target.write_bytes(encoded)


def compose(root, check=False):
    spec = data(root / BASE)
    for label, group in GROUPS.items():
        member = spec["members"]["marmish-" + label]
        prior = member["manifest"]
        if pin(root, prior["path"]) != prior:
            raise ValueError(f"predecessor member pin differs: {label}")
        package = data(root / prior["path"])
        component = {
            "kind": "public-records", "module": group["module"],
            "reexports": group["types"],
            "graph": pin(root, group["domain"] + "/graph.json"),
            "overlay": pin(root, group["domain"] + "/rust.json"),
        }
        if group["publicModule"]:
            component.update(visibility="public", rootReexports=[])
        package["components"].append(component)
        output = f"{label}-observational-records-package.json"
        write(root, output, package, check)
        member["manifest"] = pin(root, output)
    write(root, WORKSPACE, spec, check)
    return root / WORKSPACE


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    print(compose(args.portable, args.check))
