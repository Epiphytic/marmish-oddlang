"""Compose the engine error and existing identity choice into the seven-member workspace."""
import argparse
import hashlib
import json
from pathlib import Path

BASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"
WORKSPACE = "workspace-v7-engine-public-error-identity.json"
ENGINE = "engine-public-error-package.json"
NATIVE = "native-engine-error-identity-package.json"
DOMAIN = "engine-public-error-v1"


def pin(root, path):
    return {"path": path, "sha256": hashlib.sha256((root / path).read_bytes()).hexdigest()}


def data(path):
    return json.loads(path.read_bytes())


def write(root, path, value, check):
    encoded = (json.dumps(value, indent=2) + "\n").encode()
    if check:
        if (root / path).read_bytes() != encoded:
            raise ValueError(f"composed descriptor differs: {path}")
    else:
        (root / path).write_bytes(encoded)


def compose(root, check=False):
    workspace = data(root / BASE)
    engine = data(root / workspace["members"]["marmish-engine"]["manifest"]["path"])
    startup, = engine["components"]
    startup["reexports"] = ["initialize_process_limits"]
    startup["overlay"] = pin(root, f"{DOMAIN}/process-rust.json")
    engine["components"].append({
        "kind": "finite-error-domain", "module": "model",
        "reexports": ["EngineError", "NotFoundReason", "NotExecutableReason", "ResolutionReason", "Result"],
        "graph": pin(root, f"{DOMAIN}/graph.json"), "overlay": pin(root, f"{DOMAIN}/rust.json"),
    })
    write(root, ENGINE, engine, check)
    native = data(root / workspace["members"]["marmish-native"]["manifest"]["path"])
    identity, = data(root / "native-backend-choice-package.json")["components"]
    native["components"].append(identity)
    write(root, NATIVE, native, check)
    workspace["members"]["marmish-engine"]["manifest"] = pin(root, ENGINE)
    workspace["members"]["marmish-native"]["manifest"] = pin(root, NATIVE)
    write(root, WORKSPACE, workspace, check)
    return root / WORKSPACE


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    print(compose(args.portable, args.check))
