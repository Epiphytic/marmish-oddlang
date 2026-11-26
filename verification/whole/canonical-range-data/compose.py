"""Append the qualified MissingRange record to the four-type public-data candidate."""
import argparse
import hashlib
import json
from pathlib import Path

PORTABLE = Path(__file__).resolve().parents[3] / "oddlang/portable"
BEFORE = "workspace-v7-canonical-public-data"
AFTER = "workspace-v7-canonical-range-data"
ENGINE = "marmish-engine"
ENGINE_MANIFEST = "marmish-engine-canonical-range-data-package.json"
REASON = "urn:marmish:engine:output-gap:reason"


def read(name):
    return json.loads((PORTABLE / name).read_bytes())


def pin(name):
    return {"path": name, "sha256": hashlib.sha256((PORTABLE / name).read_bytes()).hexdigest()}


def checked(descriptor):
    if pin(descriptor["path"]) != descriptor:
        raise ValueError(f"stale graph, overlay or manifest {descriptor['path']}")
    return read(descriptor["path"])


def compose(check=False):
    def emit(name, value):
        path = PORTABLE / name
        encoded = (json.dumps(value, indent=2) + "\n").encode()
        if check:
            if path.read_bytes() != encoded:
                raise ValueError(f"composed input drifted: {name}")
        else:
            path.write_bytes(encoded)

    workspace = read(BEFORE + ".json")
    manifest = checked(workspace["members"][ENGINE]["manifest"])
    donor = read("engine-gap-range-package.json")
    standalone_reason, record = donor["components"]
    reason = manifest["components"][-1]
    if (
        checked(reason["graph"]) != checked(standalone_reason["graph"])
        or checked(reason["overlay"]) != checked(standalone_reason["overlay"])
        or (
            reason["kind"], reason["module"], reason["reexports"],
            record["kind"], record["module"], record["reexports"],
        ) != ("shared-declarations", "model", ["MissingReason"], "public-records", "model", ["MissingRange"])
    ):
        raise ValueError("the range component must reuse the previously qualified reason and model module")
    recovered = checked(record["graph"])
    wire_record, = recovered["modules"][0]["records"]
    if {field["type"]["external"] for field in wire_record["fields"]
        if isinstance(field["type"], dict) and "external" in field["type"]} != {REASON} or (
        checked(record["overlay"])["externalTypes"] != {REASON: ["crate", "MissingReason"]}
    ):
        raise ValueError("the range record does not use the existing public MissingReason identity")
    emit(ENGINE_MANIFEST, {**manifest, "components": [*manifest["components"], record]})
    workspace["members"][ENGINE]["manifest"] = pin(ENGINE_MANIFEST)
    emit(AFTER + ".json", workspace)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    compose(parser.parse_args().check)
