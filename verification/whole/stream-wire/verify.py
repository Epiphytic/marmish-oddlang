#!/usr/bin/env python3
"""Compare the projected wire structure with the pinned original native API."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

REVISION = "af54339d250290490d62d4475a77539b7e739071"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HERE = Path(__file__).resolve().parent
CONTRACT = HERE.parents[2] / "oddlang/portable/stream-wire-schema-v1"


def command(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def match(left, right):
    if left.read_bytes() != right.read_bytes():
        raise ValueError(f"recovered artifact differs: {right.name}")


oddlang, original, receipt = (Path(arg).resolve() for arg in sys.argv[1:])
if command("git", "rev-parse", "HEAD", cwd=original).strip() != REVISION:
    raise ValueError("unexpected original revision")
if command("git", "status", "--porcelain", cwd=original):
    raise ValueError("original checkout has tracked or untracked changes")
rust_version = command("rustc", "+1.97.1", "-vV")
if f"commit-hash: {COMPILER}" not in rust_version.splitlines():
    raise ValueError("unexpected Rust compiler")

with tempfile.TemporaryDirectory(prefix="stream-wire-reference-") as directory:
    work = Path(directory)
    observed = work / "observed"
    generated = work / "generated"
    source = original / "crates/marmish-stream/src/wire.rs"
    command(oddlang, "recover", "wire-schema", source, CONTRACT / "selection.json", observed)
    for name in ("graph.json", "rust.json", "observation.json"):
        match(observed / name, CONTRACT / name)
    command(oddlang, "build", "wire-schema", "rust", CONTRACT / "graph.json",
            CONTRACT / "rust.json", generated)
    match(generated / "projection.lock", CONTRACT / "rust.lock")
    match(generated / "obligations.json", CONTRACT / "obligations.json")

    target = work / "cargo"
    build = command("cargo", "+1.97.1", "build", "--locked", "--offline", "--release",
                    "--target-dir", target, "--message-format=json",
                    "-p", "marmish-stream", cwd=original)
    external = {}
    for line in build.splitlines():
        event = json.loads(line)
        if event["reason"] != "compiler-artifact":
            continue
        name = event["target"]["name"]
        if name in ("marmish_stream", "serde", "serde_json", "base64"):
            matches = [Path(path) for path in event["filenames"] if path.endswith(".rlib")]
            if len(matches) == 1:
                external[name] = matches[0]
    if set(external) != {"marmish_stream", "serde", "serde_json", "base64"}:
        raise ValueError("missing original and dependency libraries")
    caller = work / "caller.rs"
    caller.write_bytes((HERE / "caller.rs").read_bytes())
    (work / "projected.rs").write_bytes((generated / "wire.rs").read_bytes())
    args = [item for name, library in sorted(external.items())
            for item in ("--extern", f"{name}={library}")]
    runner = work / "compare"
    command("rustc", "+1.97.1", "--edition=2021", caller,
            "-L", f"dependency={target / 'release/deps'}", *args, "-o", runner)
    results = json.loads(command(runner))
    record = {
        "originalRevision": REVISION,
        "originalTree": command("git", "rev-parse", "HEAD^{tree}", cwd=original).strip(),
        "cargoLockSha256": digest(original / "Cargo.lock"),
        "sourceSha256": digest(source),
        "rustc": rust_version.splitlines(),
        "projection": json.loads((generated / "projection.lock").read_bytes()),
        "callerSha256": digest(HERE / "caller.rs"),
        "results": results,
    }
    receipt.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps(results, sort_keys=True))
