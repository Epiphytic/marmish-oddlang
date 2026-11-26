#!/usr/bin/env python3
"""Compare one native caller against the original and independently generated wire peers."""

import argparse
import hashlib
import io
import json
import os
import re
from pathlib import Path
import subprocess
import tarfile
import tempfile

ORIGINAL_REVISION = "af54339d250290490d62d4475a77539b7e739071"
RUSTC_REVISION = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
MANIFEST = PORTABLE / "stream-wire-package.json"


def run(*args, cwd=None, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env)


def cargo_environment(root):
    source = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    isolated = root / "cargo-home"
    isolated.mkdir()
    for name in ("registry", "git"):
        if (source / name).is_dir():
            (isolated / name).symlink_to(source / name, target_is_directory=True)
    compiler = {"RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS", "RUSTDOC",
                "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUST_TARGET_PATH"}
    environment = {key: value for key, value in os.environ.items()
                   if key not in compiler and not key.startswith("CARGO_")}
    return {**environment, "CARGO_HOME": str(isolated), "CARGO_NET_OFFLINE": "true",
            "RUSTUP_TOOLCHAIN": "1.97.1"}


def sha(value):
    return hashlib.sha256(value).hexdigest()


def digest(path):
    return sha(path.read_bytes())


def load(path):
    return json.loads(path.read_bytes())


def peer(root, target, work, env):
    libraries = {}
    command = ["cargo", "+1.97.1", "build", "--offline", "--locked", "--release",
               "--message-format=json", "--target-dir", target, "-p", "marmish-stream"]
    for line in run(*command, cwd=root, env=env).splitlines():
        item = json.loads(line)
        if item["reason"] == "compiler-artifact":
            name = item["target"]["name"]
            if name in {"marmish_stream", "serde_json"}:
                (library,) = (name for name in item["filenames"] if name.endswith(".rlib"))
                libraries[name] = library
    if set(libraries) != {"marmish_stream", "serde_json"}:
        raise ValueError("missing native compiler artifacts")
    externs = [part for name, path in sorted(libraries.items())
               for part in ("--extern", f"{name}={path}")]
    consumer = work / "consumer"
    run("rustc", "+1.97.1", "--edition=2024", "-Dwarnings", HERE / "caller.rs",
        "-L", f"dependency={target / 'release/deps'}", *externs, "-o", consumer, env=env)
    return run(consumer)


def report(raw):
    cases = []
    for line in raw.splitlines():
        case = json.loads(line)
        if isinstance(case["outcomes"], dict):
            encoded = case["outcomes"].get("encode")
            if isinstance(encoded, str) and len(encoded) > 1024:
                raw_frame = encoded.encode()
                case["outcomes"]["encode"] = {"length": len(raw_frame), "sha256": sha(raw_frame)}
        cases.append(case)
    if len({row["case"] for row in cases}) != len(cases):
        raise ValueError("native caller repeated a case ID")
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("oddlang", "original", "receipt"):
        parser.add_argument("--" + name, required=True, type=lambda value: Path(value).resolve())
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--manifest", type=lambda value: Path(value).resolve(), default=MANIFEST)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.framework_revision):
        raise ValueError("expected a full framework revision")
    original = args.original
    if run("git", "rev-parse", "HEAD", cwd=original).decode().strip() != ORIGINAL_REVISION:
        raise ValueError("unexpected original revision")
    if run("git", "status", "--porcelain", cwd=original):
        raise ValueError("original checkout has tracked or untracked changes")
    rust = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    if "commit-hash: " + RUSTC_REVISION not in rust:
        raise ValueError("unexpected rustc revision")
    with tempfile.TemporaryDirectory(prefix="native-wire-operations-") as scratch:
        root = Path(scratch)
        package = root / "package"
        environment = cargo_environment(root)
        source = root / "original"
        source.mkdir()
        archive = run("git", "archive", ORIGINAL_REVISION, cwd=original)
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            files.extractall(source, filter="data")
        run(args.oddlang, "build", "native-package", "rust", args.manifest, package, env=environment)
        package_lock = load(package / "package.lock")
        if package_lock["manifestSha256"] != digest(args.manifest):
            raise ValueError("generated package lock uses a different manifest")
        for path, expected in package_lock["generatedFiles"].items():
            if digest(package / path) != expected:
                raise ValueError(f"generated source differs from its package lock: {path}")
        original_build = root / "original-cargo"
        candidate_build = root / "candidate-cargo"
        original_raw = peer(source, original_build, root, environment)
        candidate_raw = peer(package, candidate_build, root, environment)
        if original_raw != candidate_raw:
            baseline = {row["case"]: row["outcomes"] for row in report(original_raw)}
            candidate = {row["case"]: row["outcomes"] for row in report(candidate_raw)}
            differing = sorted({*baseline, *candidate} - {
                key for key, outcome in baseline.items() if candidate.get(key) == outcome
            })
            raise ValueError(f"native wire operation disagreement: {differing}")
        record = {
            "format": "marmish-native-wire-operations-replay-v1",
            "originalRevision": ORIGINAL_REVISION,
            "originalTree": run("git", "rev-parse", "HEAD^{tree}", cwd=original).decode().strip(),
            "originalCargoLockSha256": digest(original / "Cargo.lock"),
            "originalWireSourceSha256": digest(source / "crates/marmish-stream/src/wire.rs"),
            "rustc": rust,
            "frameworkRevision": args.framework_revision,
            "oddlangBinarySha256": digest(args.oddlang),
            "packageManifestSha256": digest(args.manifest),
            "packageLock": package_lock,
            "callerSha256": digest(HERE / "caller.rs"),
            "originalOutputSha256": sha(original_raw),
            "candidateOutputSha256": sha(candidate_raw),
            "cases": report(original_raw),
        }
    args.receipt.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(f"Matched {len(record['cases'])} native wire case records")


if __name__ == "__main__":
    main()
