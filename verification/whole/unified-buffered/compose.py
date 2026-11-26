"""Add the private buffered-input member while preserving the six existing pins."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib

BASE = "workspace-v7-recovered-vault-engine-journal-mdk.json"
WORKSPACE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"
PROTOTYPE = "prototype-buffer-package.json"
MEMBER = "marmish-prototype"


def pin(root, path):
    return {"path": path.relative_to(root).as_posix(),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def requested_lock(root, descriptor):
    path = root / descriptor["path"]
    if pin(root, path) != descriptor:
        raise ValueError(f"Cargo lock does not match its pin: {path}")
    return path.read_text()


def lock_text(previous, standalone):
    old_packages = tomllib.loads(previous)["package"]
    new_packages = tomllib.loads(standalone)["package"]
    member, = (p for p in new_packages if p["name"] == MEMBER)
    provider, = (p for p in new_packages if p["name"] != MEMBER)
    if member["dependencies"] != [provider["name"]] or any(p["name"] == MEMBER for p in old_packages):
        raise ValueError("expected one new member and its single shared dependency")
    identity = ("name", "version", "source", "checksum")
    if sum(all(p.get(k) == provider[k] for k in identity) for p in old_packages) != 1:
        raise ValueError("buffer provider identity differs from the existing workspace pin")
    entry = f'[[package]]\nname = "{MEMBER}"\n'
    following = '[[package]]\nname = "marmish-stream"\n'
    if standalone.count(entry) != 1 or previous.count(following) != 1:
        raise ValueError("ambiguous package entry")
    stanza = entry + standalone.split(entry)[1].split("[[package]]")[0]
    result = previous.replace(following, stanza + following)
    if [p for p in tomllib.loads(result)["package"] if p["name"] != MEMBER] != old_packages:
        raise ValueError("predecessor Cargo package rows changed")
    return result


def compose(root, check=False):
    spec = json.loads((root / BASE).read_bytes())
    prototype = root / PROTOTYPE
    manifest = json.loads(prototype.read_bytes())
    content = lock_text(requested_lock(root, spec["cargoLock"]),
                        requested_lock(root, manifest["cargoLock"])).encode()
    output = root / WORKSPACE
    lock = output.with_suffix("") / "Cargo.lock"
    if check:
        if lock.read_bytes() != content:
            raise ValueError("workspace Cargo.lock differs beyond the new member")
    else:
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_bytes(content)
    spec["cargoLock"] = pin(root, lock)
    spec["members"][MEMBER] = {"manifest": pin(root, prototype)}
    encoded = (json.dumps(spec, indent=2) + "\n").encode()
    if check:
        if output.read_bytes() != encoded:
            raise ValueError("workspace manifest differs beyond the new member")
    else:
        output.write_bytes(encoded)
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("portable", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    print(compose(args.portable, args.check))
