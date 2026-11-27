"""Audit upstream public imports against the seven generated release libraries."""

import sys
import hashlib
import json
import os
from pathlib import Path
import subprocess
import runpy
import tempfile
from typing import NamedTuple

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
INVENTORY = HERE.parent / "public-types"
RELEASE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered"
INPUT_REVISION = "6a3eb779b60e63bef3dbfbb83e64b8a486ef77df"
RUNTIME = "59f947dafdb4cf3055e35216857472ada895a76a"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HOST = "aarch64-apple-darwin"
WORKSPACE_LOCK = "fdfa5b16d9172fe97705ee5794251c6168d0e226ea159334a52286e2e7172b68"
COUNTS = {"core": 6, "engine": 48, "mdk": 5, "native": 36, "prototype": 0, "stream": 35, "vault": 10}
product_witness = runpy.run_path(str(HERE.parent / "unified-mdk/workspace_compat.py"))["product_witness"]

if not __debug__:
    raise RuntimeError("verification requires Python assertions")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def run(argv, cwd, *, check=True):
    environment = {k: v for k, v in os.environ.items() if not k.startswith(
        ("RUST", "CARGO_BUILD_", "CARGO_ENCODED_", "CARGO_PROFILE_", "CARGO_TARGET_")
    )}
    call = subprocess.run(argv, cwd=cwd, env=dict(environment, CARGO_NET_OFFLINE="true"),
                          capture_output=True, check=False)
    if check and call.returncode:
        raise RuntimeError(call.stderr.decode(errors="replace"))
    return call


def git(root, *args):
    return run(["git", "-C", root, *args], root).stdout


class Candidate(NamedTuple):
    release: str
    input_revision: str
    framework_revision: str
    lock_sha256: str


DEFAULT_CANDIDATE = Candidate(RELEASE, INPUT_REVISION, RUNTIME, WORKSPACE_LOCK)


def verify_product(product, inputs, profile=DEFAULT_CANDIDATE):
    portable = inputs / "oddlang/portable"
    release = portable / f"{profile.release}.json"
    for path in (release, portable / profile.release / "workspace.lock"):
        assert path.read_bytes() == git(inputs, "show", f"{profile.input_revision}:{path.relative_to(inputs)}")
    locked = (product / "workspace.lock").read_bytes()
    assert sha(locked) == profile.lock_sha256
    assert locked == (portable / profile.release / "workspace.lock").read_bytes()
    workspace = json.loads(locked)
    assert sha(release.read_bytes()) == workspace["inputSha256"]
    assert set(workspace["members"]) == {f"marmish-{name}" for name in COUNTS}
    assert set(p.name for p in product.iterdir()) == set(workspace["members"]) | {
        "Cargo.toml", "Cargo.lock", "workspace.lock"}
    assert not any(path.is_symlink() for path in product.rglob("*"))
    assert sha((product / "Cargo.toml").read_bytes()) == workspace["workspaceCargoManifestSha256"]
    assert sha((product / "Cargo.lock").read_bytes()) == workspace["cargoLockSha256"]
    manifest = read(release)
    for name, member in workspace["members"].items():
        package = product / name
        pin = manifest["members"][name]["manifest"]
        assert pin["sha256"] == member["inputSha256"] == sha((portable / pin["path"]).read_bytes())
        assert sha((package / "Cargo.toml").read_bytes()) == member["workspaceCargoManifestSha256"]
        lock = package / "package.lock"
        assert sha(lock.read_bytes()) == member["standalonePackageLockSha256"]
        files = {
            p.relative_to(package).as_posix(): sha(p.read_bytes())
            for p in package.rglob("*") if p.is_file() and p not in (package / "Cargo.toml", lock)
        }
        assert files == read(lock)["generatedFiles"], name
    return manifest


def artifacts(raw, root, packages, target, *, generated):
    rows = [json.loads(line) for line in raw.splitlines()]
    assert rows[-1] == {"reason": "build-finished", "success": True}
    matches = {}
    for row in rows:
        if row["reason"] != "compiler-artifact":
            continue
        name = row["target"]["name"].replace("_", "-")
        if name not in packages or row["target"]["kind"] != ["lib"]:
            continue
        package = (root if generated else root / "crates") / name
        assert Path(row["manifest_path"]).resolve() == package / "Cargo.toml"
        assert Path(row["target"]["src_path"]).resolve().is_relative_to(package)
        assert row["profile"]["opt_level"] == "3" and not row["profile"]["test"]
        rmeta, = [Path(f).resolve() for f in row["filenames"] if f.endswith(".rmeta")]
        rlib, = [Path(f).resolve() for f in row["filenames"] if f.endswith(".rlib")]
        assert rmeta.is_relative_to(target) and rlib.is_relative_to(target)
        assert name not in matches
        matches[name] = (rmeta, rlib, row["features"], target / "release/deps", row["fresh"])
    assert matches.keys() == set(packages)
    return matches, sorted({path for row in rows if row["reason"] == "build-script-executed"
                            for path in row["linked_paths"]})


def compiler_probe(work, artifact, body, *, target=HOST):
    return product_witness("import-control", work, target, current={"role": artifact, "body": body})


def library_imports(artifacts, original, *, target=HOST):
    return product_witness("public-imports", original, target, current=artifacts)


def prototype_probe(original, candidate, original_link, candidate_link):
    return product_witness("prototype-surface", HOST, current={
        "roles": {"original": original, "generated": candidate},
        "links": {"original": original_link, "generated": candidate_link},
    })


if __name__ == "__main__":
    manifest = HERE.parent / "canonical-public-data/Cargo.toml"
    with tempfile.TemporaryDirectory(prefix="seven-public-imports-") as target:
        sys.exit(subprocess.call([
            "cargo", "+1.96.1", "run", "--quiet", "--locked", "--offline",
            "--manifest-path", manifest, "--target-dir", target,
            "--bin", "seven-public-imports", "--", *sys.argv[1:],
        ]))
