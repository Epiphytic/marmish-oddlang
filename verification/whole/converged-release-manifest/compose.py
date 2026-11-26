"""Add the existing pure parser to the native member without a new error domain."""
import argparse
import hashlib
import json
from pathlib import Path

BASE = "workspace-v7-converged-public-api"
OUTPUT = "workspace-v7-converged-release-manifest"
MEMBER = "marmish-native"
NATIVE = "native-converged-release-manifest-package.json"
BUNDLE = "release-manifest-integrated-v1"
BASE_LOCK = "3ed25cad2b1399e1c63787fedca0abbb465e758c9cfb6c635b17188a40b2dfaa"


def digest(blob):
    return hashlib.sha256(blob).hexdigest()


def pin(root, name):
    return {"path": name, "sha256": digest((root / name).read_bytes())}


def compose(root, check=False):
    def write(name, obj):
        path = root / name
        content = (json.dumps(obj, indent=2) + "\n").encode()
        if check:
            if path.read_bytes() != content:
                raise ValueError(f"integrated input differs: {name}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)

    if pin(root, f"{BASE}/workspace.lock")["sha256"] != BASE_LOCK:
        raise ValueError("wrong qualification base")
    workspace = json.loads((root / (BASE + ".json")).read_bytes())
    previous = workspace["members"][MEMBER]["manifest"]
    if pin(root, previous["path"]) != previous:
        raise ValueError("native predecessor manifest changed")
    native = json.loads((root / previous["path"]).read_bytes())
    if any(component["module"] == "native_manifest" for component in native["components"]):
        raise ValueError("native predecessor already declares the parser module")
    graph = "release-manifest-v1/graph.json"
    overlay = json.loads((root / "release-manifest-v1/rust.json").read_bytes())
    version, = json.loads((root / graph).read_bytes())["textGrammars"]
    overlay["support"] = {"nestedGrammars": {version["id"]: "version"}, "textChoices": {"target": "TARGETS"}}
    output_overlay = BUNDLE + "/rust.json"
    write(output_overlay, overlay)
    native["components"].append({
        "kind": "line-grammar",
        "module": "native_manifest",
        "graph": pin(root, graph),
        "overlay": pin(root, output_overlay),
        "reexports": [],
        "crateReexports": ["Manifest", "Asset", "version", "TARGETS"],
    })
    write(NATIVE, native)
    workspace["members"][MEMBER]["manifest"] = pin(root, NATIVE)
    write(OUTPUT + ".json", workspace)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("portable", type=Path)
    cli.add_argument("--check", action="store_true")
    args = cli.parse_args()
    compose(args.portable, args.check)
