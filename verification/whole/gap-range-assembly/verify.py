"""Assemble the recovered gap types and compare with the independently sealed package."""

import argparse
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
PRIOR = runpy.run_path(str(HERE.parent / "gap-range-recovery/verify.py"))
ROOT, PORTABLE, BASELINE, MODEL = (PRIOR[key] for key in ("ROOT", "PORTABLE", "BASELINE", "MODEL"))
run, sha, dump, read, pinned = (PRIOR[key] for key in ("run", "sha", "dump", "read", "pinned"))
FRAMEWORK = "2e2c37e53ed52676017b25e3143eca48a0daff13"
SEALED = "6355ae8504a7eb64ae01fc0591f29e3879c38128"
RECOVERED = "fb3199bcbdb334c1914228f56efa99fb00dd3d42"  # #125 selected the same recovered graph and overlay inputs.
MANIFEST = PORTABLE / "engine-gap-range-derived-package.json"
REFERENCE = PORTABLE / "engine-gap-range-recovered-package.json"
REFERENCE_LOCK = PORTABLE / "engine-gap-range-recovered-v1/package.lock"


def stage(inputs):
    inputs.mkdir()
    manifest = read(MANIFEST)
    reason, record = manifest["components"]
    if set(reason) != {"recovery", "module"} or set(record) != {"recovery"}:
        raise ValueError("compact input must derive both component kinds and exports")
    paths = {manifest["cargoLock"]["path"]: pinned(PORTABLE / manifest["cargoLock"]["path"], RECOVERED)}
    for entry in manifest["components"]:
        recovery = PORTABLE / entry["recovery"]["path"]
        if recovery.parent.name == PRIOR["RECORD"]:
            archived = HERE.parent / "gap-range-recovery/aarch64-apple-darwin/baseline-recovery.lock"
            lock = pinned(archived, SEALED)
        else:
            lock = pinned(recovery, RECOVERED)
        if recovery.read_bytes() != lock or sha(recovery) != entry["recovery"]["sha256"]:
            raise ValueError("compact input differs from the archived recovery lock")
        paths[entry["recovery"]["path"]] = lock
        for name in ("graph.json", "rust.json"):
            source = recovery.with_name(name)
            paths[source.relative_to(PORTABLE).as_posix()] = pinned(source, RECOVERED)
    for name, data in paths.items():
        path = inputs / name
        path.parent.mkdir(exist_ok=True, parents=True)
        path.write_bytes(data)
    return Path(shutil.copy2(MANIFEST, inputs))


def verify(work, args):
    native = PRIOR["PRIOR"]
    native["clean"](args.original, native["PIN"])
    companion = run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip()
    if run("git", "status", "--porcelain", "--untracked-files=no", cwd=ROOT):
        raise ValueError("expected clean tracked companion inputs")
    release = {"Darwin": "aarch64-apple-darwin", "Linux": "aarch64-unknown-linux-gnu"}[platform.system()]
    if platform.machine() not in ("arm64", "aarch64"):
        raise ValueError("expected a native ARM64 runner")
    receipt_path = HERE.parent / "gap-range-recovery" / release / "receipt.json"
    receipt = json.loads(pinned(receipt_path, SEALED))
    if receipt["host"] != release or receipt["original"] != native["PIN"]:
        raise ValueError("sealed package qualification host or original differs")
    reference = json.loads(pinned(REFERENCE_LOCK, SEALED))
    pinned(REFERENCE, SEALED)
    if (receipt["cases"]["baseline"]["packageLockSha256"] != sha(REFERENCE_LOCK)
        or receipt["cases"]["baseline"]["manifestSha256"] != sha(REFERENCE)
        or receipt["cases"]["baseline"]["generatedFiles"] != reference["generatedFiles"]):
        raise ValueError("sealed reference pins differ")

    compiler = Path(shutil.copy2(args.compiler, work / "oddlang"))
    manifest = stage(work / "inputs")
    product = work / "product"
    forbidden = {
        "original-model": args.original / MODEL,
        "original-crate-root": (args.original / MODEL).with_name("lib.rs"),
        "explicit-manifest": REFERENCE,
        "independent-package-lock": REFERENCE_LOCK,
        "native-probe": BASELINE / "probe.rs",
    }
    prefix, env = native["JOURNAL"]["sandbox"](
        native["ASSEMBLY"], work, (ROOT, args.original, args.framework, args.compiler.parent), None
    )
    for label, path in forbidden.items():
        deny = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if (
            not path.is_file() or deny.returncode != 1 or deny.stdout
            or not any(marker in deny.stderr for marker in (b"No such file", b"Operation not permitted"))
        ):
            raise ValueError(f"source isolation failed for {label}")
    command = [*prefix, compiler, "build", "native-package", "rust", manifest]
    run(*command, product, cwd=work, env=env)

    lock_path = product / "package.lock"
    lock = read(lock_path)
    spec = read(manifest)
    if lock["manifestSha256"] != sha(manifest):
        raise ValueError("assembled lock does not bind the compact manifest")
    lock["manifestSha256"] = reference["manifestSha256"]
    recoveries = []
    declarations = (PRIOR["ID"] + "reason", PRIOR["ID"] + "range")
    for component, entry, declaration in zip(lock["components"], spec["components"], declarations, strict=True):
        recovery_digest = component.pop("recoverySha256")
        recovered_declaration = component.pop("recoveredDeclaration")
        if recovery_digest != entry["recovery"]["sha256"] or recovered_declaration != declaration:
            raise ValueError("assembled lock does not bind the recovery input or stable declaration")
        recoveries.append({"sha256": recovery_digest, "declaration": recovered_declaration})
    if lock != reference:
        raise ValueError("effective descriptor or package projection differs from sealed reference")
    expected = {
        **reference["generatedFiles"],
        "Cargo.toml": reference["cargoManifestSha256"],
        "Cargo.lock": reference["cargoLockSha256"],
    }
    actual = {
        path.relative_to(product).as_posix(): sha(path)
        for path in product.rglob("*") if path.is_file() and path != lock_path
    }
    if actual != expected:
        raise ValueError("generated package files differ from sealed reference")

    range_lock = "engine-gap-range-recovered-v1/recovery.lock"
    graph_file = "engine-gap-range-recovered-v1/graph.json"
    diagnostics = {}
    for path in (graph_file, range_lock):
        changed = manifest.parent / path
        original = changed.read_bytes()
        changed.write_bytes(original + b"\n")
        rejected_path = work / "rejected"
        rejected = subprocess.run([*command, rejected_path], cwd=work, env=env, capture_output=True)
        diagnostics[path] = rejected.stderr.decode().strip()
        if rejected.returncode == 0 or rejected_path.exists() or diagnostics[path] != f"input digest mismatch: {path}":
            raise ValueError(f"package failed to reject tampered input: {path}")
        changed.write_bytes(original)

    return {
        "format": "marmish-gap-range-derived-package-v1",
        "host": release,
        "compilerSha256": sha(compiler),
        "framework": FRAMEWORK,
        "frameworkBuildToolchain": args.framework_toolchain,
        "companion": companion,
        "verifierSha256": sha(HERE / "verify.py"),
        "sealedCompanion": SEALED,
        "referenceReceiptSha256": sha(receipt_path),
        "referencePackageLockSha256": sha(REFERENCE_LOCK),
        "manifestSha256": sha(manifest),
        "assembledPackageLockSha256": sha(lock_path),
        "recoveries": recoveries,
        "components": reference["components"],
        "files": actual,
        "deniedReads": sorted(forbidden),
        "tamperDiagnostics": diagnostics,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler_target", "original", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    PRIOR["clean"](args.framework, FRAMEWORK)
    args.framework_toolchain = run("rustc", "+1.96.1", "-vV").decode().splitlines()
    if f"commit-hash: {PRIOR['BUILD_RUSTC']}" not in args.framework_toolchain:
        raise ValueError("expected the pinned Oddlang build toolchain")
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"):
        env.pop(key, None)
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=env)
    args.compiler = args.compiler_target / "debug/oddlang"
    with tempfile.TemporaryDirectory(prefix="odd-gap-range-assembly-") as temp:
        dump(args.receipt, verify(Path(temp).resolve(), args))
