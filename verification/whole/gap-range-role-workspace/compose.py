"""Select the exact checked native-role binding in the recovered seven-member workspace."""
import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BASE = "workspace-v7-canonical-recovered-range"
WORKSPACE = "workspace-v7-canonical-range-native-idioms"
ENGINE = "marmish-engine"
PACKAGE = f"{ENGINE}-canonical-range-native-idioms-package.json"
DONOR = "engine-gap-range-native-idioms-package.json"
ROLE_BINDING = "engine-gap-range-native-idioms-v1/rust.json"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(name):
    return json.loads((PORTABLE / name).read_bytes())


def pin(name):
    return {"path": name, "sha256": sha((PORTABLE / name).read_bytes())}


def compose(check=False):
    workspace = read(f"{BASE}.json")
    member = workspace["members"][ENGINE]
    before = member["manifest"]
    if before != pin(before["path"]):
        raise ValueError("wrong recovered workspace engine pin")
    package = read(before["path"])
    range_component = package["components"][-1]
    donor_reason, donor_range = read(DONOR)["components"]
    if donor_reason != package["components"][-2] or donor_range != {**range_component, "overlay": pin(ROLE_BINDING)}:
        raise ValueError("role-checked donor must change only the recovered range Rust binding")
    if donor_range == range_component or range_component["reexports"] != ["MissingRange"]:
        raise ValueError("expected an explicit binding replacement for public MissingRange")
    package["components"][-1] = donor_range

    def render(name, data):
        raw = (json.dumps(data, indent=2) + "\n").encode()
        path = PORTABLE / name
        if check:
            if raw != path.read_bytes():
                raise ValueError(f"composition differs at {name}")
        else:
            path.write_bytes(raw)
        return {"path": name, "sha256": sha(raw)}

    member["manifest"] = render(PACKAGE, package)
    render(f"{WORKSPACE}.json", workspace)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    compose(parser.parse_args().check)
