"""Derive the six-member inputs while preserving the five existing member pins."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib

BASE = "workspace-v7-recovered-vault-engine-journal.json"
WORKSPACE = "workspace-v7-recovered-vault-engine-journal-mdk.json"
MDK = "mdk-public-data-package.json"
MEMBER = "marmish-mdk"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def data(path):
    return json.loads(path.read_bytes())


def pin(root, path):
    return {"path": str(path.relative_to(root)), "sha256": sha(path)}


def lock_text(old, mdk):
    entries = tomllib.loads(mdk)["package"]
    expected = {"name": MEMBER, "version": "0.1.0"}
    if entries != [expected] or any(row["name"] == MEMBER for row in tomllib.loads(old)["package"]):
        raise ValueError("expected a single, dependency-free MDK package")
    entry = '[[package]]\nname = "marmish-native"\n'
    if old.count(entry) != 1:
        raise ValueError("expected one existing native package entry")
    return old.replace(entry, f'[[package]]\nname = "{MEMBER}"\nversion = "0.1.0"\n\n' + entry)


def compose(root, check=False):
    existing = data(root / BASE)
    path = root / existing["cargoLock"]["path"]
    if pin(root, path) != existing["cargoLock"]:
        raise ValueError("predecessor Cargo lock does not match its pin")
    descriptor = data(root / MDK)["cargoLock"]
    member_lock = root / descriptor["path"]
    if pin(root, member_lock) != descriptor:
        raise ValueError("MDK package Cargo lock does not match its pin")
    lock = root / Path(WORKSPACE).stem / "Cargo.lock"
    lock_bytes = lock_text(path.read_text(), member_lock.read_text()).encode()
    if check:
        if lock.read_bytes() != lock_bytes:
            raise ValueError("six-member Cargo lock differs beyond the MDK addition")
    else:
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_bytes(lock_bytes)
    existing["cargoLock"] = pin(root, lock)
    existing["members"][MEMBER] = {"manifest": pin(root, root / MDK)}
    manifest = root / WORKSPACE
    output = (json.dumps(existing, indent=2) + "\n").encode()
    if check and manifest.read_bytes() != output:
        raise ValueError("six-member workspace differs beyond the MDK addition")
    if not check:
        manifest.write_bytes(output)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    print(compose(args.portable, args.check))
