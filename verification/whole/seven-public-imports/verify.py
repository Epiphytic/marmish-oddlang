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
    return product_witness("import-manifest", product, inputs, profile.release,
                           profile.input_revision, profile.lock_sha256)


def artifacts(raw, root, packages, target, *, generated):
    package_root = root if generated else root / "crates"
    log = raw.decode() if isinstance(raw, bytes) else raw
    roles, links = product_witness("import-artifacts", package_root, target, *packages, current=log)
    return {
        name: (Path(meta), Path(lib), features, Path(deps), fresh)
        for name, (meta, lib, features, deps, fresh) in roles.items()
    }, links


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
