"""Append the tested MDK byte parser to the locked offline workspace."""
import argparse
import json
from pathlib import Path
import tomllib

from verify import MANIFEST, PORTABLE, pin

BASE = "workspace-v7-canonical-offline.json"
OUTPUT = "workspace-v7-canonical-mdk-test-seam.json"
PACKAGE = "mdk-json-test-package.json"
MEMBER = "marmish-mdk"


def emit(path, content, check):
    if check:
        if path.read_bytes() != content:
            raise ValueError(f"composed file differs: {path.name}")
    else:
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(content)


def checked(path, descriptor):
    contents = path.read_bytes()
    if pin(PORTABLE, path) != descriptor:
        raise ValueError(f"input pin differs: {path.name}")
    return contents


def add_mdk_dev_edge(lock):
    old = tomllib.loads(lock.decode())
    entry, = [row for row in old["package"] if row["name"] == MEMBER]
    if entry != {"name": MEMBER, "version": "0.1.0"}:
        raise ValueError("the canonical MDK lock entry is no longer dependency-free")
    marker = b'name = "marmish-mdk"\nversion = "0.1.0"\n'
    if lock.count(marker) != 1:
        raise ValueError("ambiguous canonical MDK lock row")
    result = lock.replace(marker, marker + b'dependencies = [\n "serde_json",\n]\n')
    entry["dependencies"] = ["serde_json"]
    if tomllib.loads(result.decode()) != old:
        raise ValueError("composition changed another Cargo entry")
    return result


def compose(check):
    workspace = json.loads((PORTABLE / BASE).read_bytes())
    descriptor = workspace["members"][MEMBER]["manifest"]
    prior = json.loads(checked(PORTABLE / descriptor["path"], descriptor))
    bounded = json.loads((PORTABLE / MANIFEST).read_bytes())
    component = bounded["components"][-1]
    if (
        bounded["components"] != [*prior["components"], component]
        or component.get("kind") != "bounded-json-input"
        or component.get("reexports") != []
        or bounded["dependencies"] != {"serde_json": {"version": "1.0.151"}}
    ):
        raise ValueError("standalone MDK parser input differs from the canonical member")
    for field in ("graph", "overlay"):
        checked(PORTABLE / component[field]["path"], component[field])

    def emit_json(path, value):
        emit(path, (json.dumps(value, indent=2, sort_keys=True) + "\n").encode(), check)

    package = dict(prior, components=[*prior["components"], component],
                   cargoLock=bounded["cargoLock"], devDependencies=bounded["dependencies"])
    checked(PORTABLE / package["cargoLock"]["path"], package["cargoLock"])
    path = PORTABLE / PACKAGE
    emit_json(path, package)
    workspace["members"][MEMBER]["manifest"] = pin(PORTABLE, path)
    descriptor = workspace["cargoLock"]
    path = PORTABLE / Path(OUTPUT).stem / "Cargo.lock"
    emit(path, add_mdk_dev_edge(checked(PORTABLE / descriptor["path"], descriptor)), check)
    workspace["cargoLock"] = pin(PORTABLE, path)
    emit_json(PORTABLE / OUTPUT, workspace)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true")
    compose(ap.parse_args().check)
