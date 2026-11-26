"""Append the qualified engine data choices to the canonical workspace inputs."""
import argparse
import hashlib
import json
from pathlib import Path

PORTABLE = Path(__file__).resolve().parents[3] / "oddlang/portable"
BEFORE = PORTABLE / "workspace-v7-canonical-offline.json"
CANDIDATE = PORTABLE / "workspace-v7-canonical-engine-choices.json"
ENGINE = "marmish-engine"
ENGINE_CHOICES = PORTABLE / "engine-fieldless-choices-package.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compose():
    workspace = json.loads(BEFORE.read_bytes())
    slot = workspace["members"][ENGINE]["manifest"]
    predecessor = PORTABLE / slot["path"]
    if digest(predecessor) != slot["sha256"]:
        raise ValueError("canonical engine manifest changed")
    old = json.loads(predecessor.read_bytes())
    new = json.loads(ENGINE_CHOICES.read_bytes())
    appended = new["components"][-1]
    if new != {**old, "components": [*old["components"], appended]}:
        raise ValueError("engine composition changed inherited inputs")
    if (appended["kind"], appended["module"], appended["reexports"]) != (
        "checked-values", "model", ["Stream", "SubmitDisposition"]
    ):
        raise ValueError("engine input must append only the two fieldless data choices")
    workspace["members"][ENGINE]["manifest"] = {
        "path": ENGINE_CHOICES.name, "sha256": digest(ENGINE_CHOICES),
    }
    return (json.dumps(workspace, indent=2) + "\n").encode()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    if parser.parse_args().check:
        if CANDIDATE.read_bytes() != compose():
            raise ValueError("candidate workspace input differs from canonical composition")
    else:
        CANDIDATE.write_bytes(compose())
