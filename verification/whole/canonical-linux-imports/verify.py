"""Compile the frozen Darwin import corpus against both native Linux ARM64 libraries."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
audit_spec = importlib.util.spec_from_file_location(
    "seven_public_imports", HERE.parent / "seven-public-imports/verify.py")
audit = importlib.util.module_from_spec(audit_spec)
audit_spec.loader.exec_module(audit)

HOST = "aarch64-unknown-linux-gnu"
CANONICAL = audit.Candidate(
    "workspace-v7-canonical-offline",
    "0f9e57396530fa1c11127e3fff7d8d0018514f00",
    "8a20ea283d0d43da42f054a7c7774eb9e6a20da7",
    "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4",
)
DARWIN_EVIDENCE = "55da1b628138d48d1984f87f173a061886d48ea8"
DARWIN = HERE.parent / "canonical-workspace/aarch64-apple-darwin.json"
NAMES = [f"marmish-{name}" for name in audit.COUNTS]


def file_sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def source_pin(original):
    return audit.product_witness("import-source-pin", original)


def frozen_inputs(inputs):
    audit.product_witness("frozen-import-inventory", inputs)


def build(root, target):
    # Execute Cargo afresh even for an existing target. Cargo's own build decisions
    # and emitted artifact paths are used instead of trusting an external log.
    command = [
        "cargo",
        "+1.97.1",
        "build",
        "--release",
        "--workspace",
        "--lib",
        "--target",
        HOST,
        "--locked",
        "--offline",
        "--target-dir",
        str(target),
        "--message-format=json",
    ]
    env = {
        key: os.environ[key]
        for key in ("HOME", "PATH", "CARGO_HOME", "TMPDIR")
        if key in os.environ
    }
    return subprocess.check_output(
        command, cwd=root, env=env | {"CARGO_NET_OFFLINE": "true", "LC_ALL": "C"}
    )


def closure(raw, target, host=HOST):
    log = raw if isinstance(raw, str) else raw.decode()
    return audit.product_witness("library-closure", target, host, current=log)


def import_rows(role, root, target, original, *, generated):
    raw = build(root, target)
    artifacts, _ = audit.artifacts(raw, root, NAMES, target, generated=generated)
    release = target / HOST / "release"
    for artifact in artifacts.values():
        assert all(path.is_relative_to(release) for path in artifact[:2])
    before = closure(raw, target)
    rows = audit.library_imports(artifacts, original, target=HOST)
    controls = {}
    with tempfile.TemporaryDirectory(prefix="linux-import-controls-") as temp:
        for label, path, code in (
            ("absent", "NoSuchAuditSentinel", "E0432"),
            ("private", "config", "E0603"),
        ):
            probe = audit.compiler_probe(
                Path(temp),
                artifacts["marmish-core"],
                f"pub use observed::{path} as witness;",
                target=HOST,
            )
            assert not probe["compiles"] and [
                row["code"] for row in probe["diagnostics"]
            ] == [code]
            controls[label] = probe
    assert before == closure(raw, target), (
        f"{role} Cargo closure changed while probing imports"
    )
    return {
        "cargoArtifactLogSha256": audit.sha(raw),
        "cargoArtifactClosure": before,
        "negativeImportControls": controls,
        "libraries": rows,
    }


def summary(original, generated, darwin):
    return audit.product_witness("compare-platform-imports", current={
        "originalLinux": original, "generatedLinux": generated, "generatedDarwin": darwin,
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "product",
        "generated_target",
        "inputs",
        "original",
        "original_target",
        "receipt",
    ):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    version = audit.run(["rustc", "+1.97.1", "-vV"], HERE).stdout.decode().splitlines()
    assert f"host: {HOST}" in version and f"commit-hash: {audit.COMPILER}" in version
    assert args.generated_target != args.original_target
    for target in (args.generated_target, args.original_target):
        assert not target.is_relative_to(args.product) and not target.is_relative_to(
            args.original
        )
    source = source_pin(args.original)
    frozen_inputs(args.inputs)
    manifest = audit.verify_product(args.product, args.inputs, CANONICAL)
    original = import_rows(
        "original", args.original, args.original_target, args.original, generated=False
    )
    generated = import_rows(
        "generated", args.product, args.generated_target, args.original, generated=True
    )
    darwin = audit.read(DARWIN)["darwinPublicImports"]["libraries"]
    results = summary(original["libraries"], generated["libraries"], darwin)
    assert source_pin(args.original) == source
    assert audit.verify_product(args.product, args.inputs, CANONICAL) == manifest
    frozen_inputs(args.inputs)
    workspace = audit.read(args.product / "workspace.lock")
    report = {
        "format": "oddlang-canonical-linux-import-audit-v1",
        "rustc": version,
        "cargo": audit.run(["cargo", "+1.97.1", "-vV"], HERE)
        .stdout.decode()
        .splitlines(),
        "candidate": CANONICAL._asdict(),
        "candidateWorkspaceManifestSha256": workspace["inputSha256"],
        "candidateCargoLockSha256": workspace["cargoLockSha256"],
        "candidatePackageLocks": {
            name: row["standalonePackageLockSha256"]
            for name, row in workspace["members"].items()
        },
        "original": source,
        "darwinEvidenceSha256": file_sha(DARWIN),
        "darwinInventoryProvenanceSha256": file_sha(
            audit.INVENTORY / "provenance.json"
        ),
        "verifierSha256": file_sha(Path(__file__)),
        "sharedVerifierSha256": file_sha(Path(audit.__file__)),
        "originalBuild": original,
        "generatedBuild": generated,
        **results,
    }
    args.receipt.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
