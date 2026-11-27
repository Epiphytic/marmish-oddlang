"""Reconcile same-profile core declaration observations and verify their input bytes."""

import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
RUST_OBSERVER = "b832a8cd9414f15a2c698761029cb76044da9a8f"
OBSERVER = "d32e179075a660fffa0d3f0905bbe8ae8957c570"
PROJECTION = "8a20ea283d0d43da42f054a7c7774eb9e6a20da7"
ROOT = Path(__file__).resolve().parents[3]
PORTABLE = ROOT / "oddlang/portable"
WORKSPACE = PORTABLE / "workspace-v7-canonical-offline.json"
WORKSPACE_LOCK = WORKSPACE.with_suffix("") / "workspace.lock"
PACKAGE = PORTABLE / "core-package.json"
WORKSPACE_SHA = "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4"
CORE = "marmish-core"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path):
    return json.loads(path.read_bytes())


def save(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def pinned_git(path, revision):
    actual = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=path, text=True
    ).strip()
    require(actual == revision, f"unexpected Git revision in {path}")
    status = subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=path
    )
    require(not status, f"expected a clean checkout at {path}")


@contextmanager
def observer_command(checkout, historical_revision):
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
    require(revision in (RUST_OBSERVER, historical_revision), f"unexpected observer revision in {checkout}")
    pinned_git(checkout, revision)
    if revision == historical_revision:
        yield revision, ["python3", "-B", checkout / "verification/rust-recovery/release_rustdoc.py"]
    else:
        manifest = checkout / "verification/provider-validation/Cargo.toml"
        with tempfile.TemporaryDirectory(prefix="rustdoc-observer-build-") as target:
            yield revision, ["cargo", "+1.96.1", "run", "--quiet", "--locked", "--offline",
                             "--target-dir", target, "--manifest-path", manifest, "--bin", "release-rustdoc", "--"]
    pinned_git(checkout, revision)


def snapshot(candidate):
    require(sha(WORKSPACE_LOCK) == WORKSPACE_SHA, "canonical lock changed")
    member = load(WORKSPACE_LOCK)["members"][CORE]
    manifest_sha = sha(PACKAGE)
    require(manifest_sha == member["inputSha256"], "core package input changed")
    workspace = load(WORKSPACE)
    require(
        sha(WORKSPACE) == load(WORKSPACE_LOCK)["inputSha256"],
        "canonical manifest changed",
    )
    require(
        workspace["members"][CORE]["manifest"]
        == {"path": PACKAGE.name, "sha256": manifest_sha},
        "canonical workspace points to a different core manifest",
    )
    layout = load(PACKAGE)
    for pin in [
        layout["cargoLock"],
        *(row[k] for row in layout["components"] for k in ("graph", "overlay")),
    ]:
        require(
            sha(PORTABLE / pin["path"]) == pin["sha256"],
            f"core input changed: {pin['path']}",
        )
    package_lock = candidate / "package.lock"
    require(
        sha(package_lock) == member["standalonePackageLockSha256"],
        "standalone package lock changed",
    )
    lock = load(package_lock)
    require(
        lock["manifestSha256"] == manifest_sha,
        "package lock points to another manifest",
    )
    expected = {
        "package.lock": sha(package_lock),
        "Cargo.toml": lock["cargoManifestSha256"],
        "Cargo.lock": lock["cargoLockSha256"],
        **lock["generatedFiles"],
    }
    files = list(candidate.rglob("*"))
    require(
        all(not p.is_symlink() and (p.is_file() or p.is_dir()) for p in files),
        "generated package includes an unexpected filesystem entry",
    )
    actual = {p.relative_to(candidate).as_posix(): sha(p) for p in files if p.is_file()}
    require(actual.keys() == expected.keys(), "unexpected generated file inventory")
    for name, digest in actual.items():
        require(digest == expected[name], f"generated file changed: {name}")
    return {
        "workspaceManifestSha256": sha(WORKSPACE),
        "workspaceLockSha256": WORKSPACE_SHA,
        "coreManifestSha256": manifest_sha,
        "generatedFiles": actual,
    }


def collect(observer, original, candidate, triple, output):
    pinned_git(original, ORIGINAL)
    before = snapshot(candidate)
    with observer_command(observer, OBSERVER) as (revision, command):
        for name, source in (("original", original), ("generated", candidate)):
            argv = [
                *command,
                source,
                CORE,
                "lib",
                "marmish_core",
                triple,
                "--out",
                output / f"{name}.json",
            ]
            if name == "original":
                argv += ["--revision", ORIGINAL]
            subprocess.run(argv, check=True)
    after = snapshot(candidate)
    require(before == after, "candidate changed while observing")
    pinned_git(original, ORIGINAL)
    # Mutate a disposable copy only. The package lock stays fixed.
    with tempfile.TemporaryDirectory(prefix="core-provenance-") as scratch:
        edited = Path(scratch)
        for name in before["generatedFiles"]:
            target = edited / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((candidate / name).read_bytes())
        source = edited / "src/lib.rs"
        source.write_bytes(source.read_bytes() + b"\n// provenance control\n")
        try:
            snapshot(edited)
        except ValueError as failure:
            rejection = str(failure)
            require(rejection == "generated file changed: src/lib.rs", rejection)
        else:
            raise ValueError("changed source accepted under original package lock")
    save(
        output / "receipt.json",
        {
            "host": triple,
            "originalRevision": ORIGINAL,
            "observerRevision": revision,
            "projectionRevision": PROJECTION,
            "before": before,
            "after": after,
            "mutation": {"file": "src/lib.rs", "outcome": rejection},
            "reportsSha256": {
                name: sha(output / f"{name}.json") for name in ("original", "generated")
            },
        },
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("observer", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument(
        "host", choices=("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")
    )
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir()
    collect(
        args.observer.resolve(),
        args.original.resolve(),
        args.candidate.resolve(),
        args.host,
        args.output.resolve(),
    )


if __name__ == "__main__":
    main()
