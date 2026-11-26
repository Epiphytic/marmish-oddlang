"""Select the native source-recovered reason and range in the seven-member product."""
import argparse
import hashlib
import json
from pathlib import Path

PORTABLE = Path(__file__).resolve().parents[3] / "oddlang/portable"
BASE, OUTPUT = "workspace-v7-canonical-range-data", "workspace-v7-canonical-recovered-range"
ENGINE = "marmish-engine"
PACKAGE = "marmish-engine-canonical-recovered-range-package.json"
DONOR = "engine-gap-range-recovered-package.json"
REASON, RANGE = "engine-gap-reason-recovered-v1", "engine-gap-range-recovered-v1"
AUTHORED = {"engine-gap-reason-v1", "engine-gap-range-v1",
            "engine-gap-reason-package.json", "engine-gap-range-package.json"}


def pin(path):
    return {"path": path, "sha256": hashlib.sha256((PORTABLE / path).read_bytes()).hexdigest()}


def read(path):
    return json.loads((PORTABLE / path).read_bytes())


def check_pin(descriptor):
    name = descriptor["path"]
    if pin(name) != descriptor:
        raise ValueError(f"wrong portable digest for {name}")
    return read(name)


def compose(check=False):
    def write(name, payload):
        output = (json.dumps(payload, indent=2) + "\n").encode()
        path = PORTABLE / name
        if check:
            if path.read_bytes() != output:
                raise ValueError(f"composition differs at {name}")
        else:
            path.write_bytes(output)

    base = read(BASE + ".json")
    engine = check_pin(base["members"][ENGINE]["manifest"])
    donor_reason, donor_range = read(DONOR)["components"]
    before_reason, before_range = engine["components"][-2:]
    for component, folder, role, expected_kind in (
        (donor_reason, REASON, "MissingReason", "shared-declarations"),
        (donor_range, RANGE, "MissingRange", "public-records"),
    ):
        if (component["kind"], component["module"], component["reexports"]) != (
            expected_kind, "model", [role],
        ):
            raise ValueError(f"unexpected recovered component: {role}")
        for key, filename in (("graph", "graph.json"), ("overlay", "rust.json")):
            if component[key] != pin(f"{folder}/{filename}"):
                raise ValueError(f"recovered {role} must use the published {filename} bytes")
    if before_reason != donor_reason:
        raise ValueError("the already recovered reason component changed")
    if {k: v for k, v in before_range.items() if k not in ("graph", "overlay")} != {
        k: v for k, v in donor_range.items() if k not in ("graph", "overlay")
    } or any(check_pin(before_range[key]) != check_pin(donor_range[key]) for key in ("graph", "overlay")):
        raise ValueError("recovered range differs from its authored data contract")
    if any(before_range[key]["sha256"] == donor_range[key]["sha256"] for key in ("graph", "overlay")):
        raise ValueError("expected the range input paths and raw digests to change")

    write(PACKAGE, {**engine, "components": [*engine["components"][:-1], donor_range]})
    base["members"][ENGINE]["manifest"] = pin(PACKAGE)
    write(OUTPUT + ".json", base)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true")
    compose(p.parse_args().check)
