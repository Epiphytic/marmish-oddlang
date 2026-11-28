"""Check the seven-member engine-choice candidate using native release metadata and data callers."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WHOLE = HERE.parent
PORTABLE = ROOT / "oddlang/portable"
CANDIDATE = PORTABLE / "workspace-v7-canonical-engine-choices.json"
ENGINE = "marmish-engine"
NATIVE = runpy.run_path(str(WHOLE / "unified-mdk/workspace_compat.py"))
digest = NATIVE["sha"]
FRAMEWORK = "8e3320aacb0106d543c3e21ab1faf60a02132c04"
INPUT_REVISION = "934bd7c96af85e0c502cf943b1c1d72f36692a8e"
LOCK_SHA256 = "a72d0b0538b3afc272fcd7f5cf8cc900935fd4cc920261fa4bd6bcea46270292"
STANDALONE_PIN = "042f018252c9ed04bea97e10f7da1df16da1e0b0"
RECOVERY_PIN = "7fcbdfc396e0a5ea4c7b4f691dfea965e0244919"
ENGINE_STANDALONE = WHOLE / "engine-fieldless-choices"
LOCK = CANDIDATE.with_suffix("") / "workspace.lock"


def pin_bytes(path, revision):
    want = subprocess.check_output(
        ["git", "show", f"{revision}:{path.relative_to(ROOT)}"], cwd=ROOT
    )
    if want != path.read_bytes():
        raise ValueError(f"pinned input changed: {path}")
    return want


VAULT_OBSERVER_PIN = "50b38f6113b553797e6e51871af4d2f4e5b13ba3"
WORKSPACE_TOOLING_PIN = "b0cd9f99ad03d87054420fcaca9d4964bdd9d7d6"
VAULT_MODE_PIN = "ff41a8961c5de372e15f0067705920a95eb7fb31"
BUILD_HOST_PIN = "0c044a81304af090567eec0b70c56614833c378a"
HOST_ADAPTER_PIN = "95db5976653db21e48457b4a1f2b2d6cefd08bbc"
ENGINE_SERDE_PIN = "0efc1e0a4e073ba8a50a2bf34e5554a71a73b7fe"
REFERENCE_PIN = "b4ce5421ff0c1eab7f454ab514260cae47633e0e"
ENGINE_SNAPSHOT_PIN = "cb59e073e75116b37d72eae545ce0ea2a1c17d3d"
PLATFORM_IMPORT_PIN = "7519e0085608a9e16d3a5bab2e7ef96052774a23"
ROLE_AUDIT_PIN = "07759feb808eff49a59577b5a554f464b5ea1352"
LINUX_AUDIT_PIN = "16c6c7d3be3ca1792ae8fa0e43eacb8bae7af606"
INHERITED_IMPORTS_PIN = "d6b6afc91b06d86dd92efdc509c252b4f0c1a9da"
HELPER_PINS = {
    WHOLE / "seven-public-imports/inherited.rs": INHERITED_IMPORTS_PIN,
    WHOLE / "canonical-linux-imports/role.rs": ROLE_AUDIT_PIN,
    WHOLE / "unified-mdk/workspace_compat.py": ROLE_AUDIT_PIN,
    WHOLE / "canonical-linux-imports/summary.rs": PLATFORM_IMPORT_PIN,
    ENGINE_STANDALONE / "qualification.rs": "82342ef7cf85f5e6cf6291ec3eb171de320b2b9c",
    ENGINE_STANDALONE / "build-host.rs": BUILD_HOST_PIN,
    WHOLE / "canonical-public-data/src/build_host.rs": BUILD_HOST_PIN,
    ENGINE_STANDALONE / "observer.rs": RECOVERY_PIN,
    WHOLE / "canonical-public-data/src/bin/engine-choice-inputs.rs": REFERENCE_PIN,
    ENGINE_STANDALONE / "probe.rs": STANDALONE_PIN,
    WHOLE / "canonical-public-data/src/bin/vault-mode-observer.rs": VAULT_MODE_PIN,
    WHOLE / "vault-mode/observe.rs": VAULT_MODE_PIN,
    WHOLE / "vault-fixed-bytes/witness.rs": VAULT_OBSERVER_PIN,
    WHOLE / "core-package/verify.py": HOST_ADAPTER_PIN,
    WHOLE / "workspace/verify.py": WORKSPACE_TOOLING_PIN,
    WHOLE / "canonical-linux-imports/verify.rs": LINUX_AUDIT_PIN,
    WHOLE / "canonical-public-data/imports.rs": LINUX_AUDIT_PIN,
    WHOLE / "canonical-public-data/witness.rs": INHERITED_IMPORTS_PIN,
    WHOLE / "canonical-public-data/product.rs": ENGINE_SNAPSHOT_PIN,
    WHOLE / "canonical-public-data/src/artifacts.rs": ENGINE_SERDE_PIN,
}
HELPER_HASHES = {
    path.relative_to(ROOT).as_posix(): hashlib.sha256(
        pin_bytes(path, revision)
    ).hexdigest()
    for path, revision in HELPER_PINS.items()
}
# The earlier standalone run used the journal projection embedded in its Python
# verifier. Current replay loads that helper and its native projection sources.
JOURNAL_INPUT_PINS = {
    "journal-records/verify.py": "2bb91a8849fc8cf66293fc31de3d50d566a6e8319d8e33b2f0d6381a5de725d8",
    "journal-records/recovery.rs": "1acceb5f9260cca04a7b5989fadaaa0f9f21d5a82087bd2bccaecf0bdd11e682",
    "journal-records/witness.rs": "c1061086c30ff0b806b694b094082a1748eb66e9b2de5d6e3e4585d42e6c2ca7",
    "canonical-public-data/src/bin/journal-record-inputs.rs": "cd78ccc0f484a7bcb5bc33c76e6e7867f5a2bc7df6e1992cceaa3bebd5a9a2ce",
}
for relative, expected in JOURNAL_INPUT_PINS.items():
    path = WHOLE / relative
    actual_digest = digest(path)
    if actual_digest != expected:
        raise ValueError(f"pinned journal input helper changed: {path}")
    HELPER_HASHES[path.relative_to(ROOT).as_posix()] = actual_digest

# Earlier receipts bind this deleted verifier to its release commit.
ARCHIVED_COMPOSER = "verification/whole/canonical-engine-choices/compose.py"
ARCHIVED_COMPOSER_SHA = hashlib.sha256(subprocess.check_output(
    ["git", "show", f"{INPUT_REVISION}:{ARCHIVED_COMPOSER}"], cwd=ROOT)).hexdigest()
if not __debug__:
    raise RuntimeError("verification requires Python assertions")

DARWIN = "aarch64-apple-darwin"
LINUX = "aarch64-unknown-linux-gnu"


CORE = runpy.run_path(str(WHOLE / "core-package/verify.py"))


def observe(tool, library, serde, host_deps, links, host, work, env):
    args = [tool, "observe", library, serde, host_deps, host, work]
    for link in links:
        args.extend(("--link", link))
    return json.loads(subprocess.check_output(args, env=env))


def pinned(path, commit):
    return json.loads(pin_bytes(path, commit))


def snapshots(args):
    return NATIVE["product_witness"](
        "engine-choice-snapshot", args.baseline, args.product, INPUT_REVISION, LOCK_SHA256
    )


def imports(host, artifacts, original):
    libraries = NATIVE["product_witness"]("public-imports", original, host, current=artifacts)
    return NATIVE["product_witness"]("compare-engine-imports", current=[NATIVE["product_witness"]("canonical-import-baseline", host), libraries])


def candidate_serde(raw, target, host):
    log = raw.decode() if isinstance(raw, bytes) else raw
    return Path(NATIVE["product_witness"]("engine-serde", target, host, current=log))


def qualify(args):
    assert args.input_revision == INPUT_REVISION and digest(LOCK) == LOCK_SHA256
    tool = CORE["qualification_tool"]("engine-choice-inputs")[1]
    companion = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    subprocess.run([tool, "checkout", ROOT, companion], check=True)
    subprocess.run([tool, "checkout", args.framework, FRAMEWORK], check=True)
    original = NATIVE["product_witness"]("import-source-pin", args.original)
    assert all(
        not target.is_relative_to(root)
        for target in (args.target, args.original_target)
        for root in (args.product, args.baseline, args.original, args.framework, ROOT)
    )
    assert args.target != args.original_target
    build_overrides = ("RUST", "CARGO_BUILD_", "CARGO_ENCODED_", "CARGO_PROFILE_", "CARGO_TARGET_")
    environment = {key: value for key, value in os.environ.items() if not key.startswith(build_overrides)}
    rustc = subprocess.check_output(
        ["rustc", "+1.97.1", "-vV"], cwd=HERE, env={**environment, "CARGO_NET_OFFLINE": "true"},
        text=True,
    ).splitlines()
    (host,) = (line[6:] for line in rustc if line.startswith("host: "))
    assert host in (DARWIN, LINUX) and os.uname().machine == (
        "arm64" if host == DARWIN else "aarch64"
    )
    assert f"commit-hash: {CORE['RUST_COMMIT']}" in rustc
    standalone = pinned(ENGINE_STANDALONE / f"{host}.json", STANDALONE_PIN)
    assert standalone["framework"] == FRAMEWORK and standalone[
        "compilerSha256"
    ] == digest(args.compiler)
    NATIVE["product_witness"]("frozen-import-inventory", ROOT)
    archived_verifier = subprocess.check_output(
        ["git", "show", f"{STANDALONE_PIN}:verification/whole/engine-fieldless-choices/verify.py"], cwd=ROOT)
    assert hashlib.sha256(archived_verifier).hexdigest() == standalone["verifierSha256"]
    assert digest(ENGINE_STANDALONE / "probe.rs") == standalone["probeSha256"]
    before = snapshots(args)
    with tempfile.TemporaryDirectory(
        prefix="canonical-engine-choice-observer-"
    ) as scratch:
        work = Path(scratch).resolve()
        env = CORE["host_build_env"](work / "cargo-home")
        built = NATIVE["product_witness"]("build-import-libraries", args.product, args.target, host)
        roles, links = NATIVE["product_witness"]("import-artifacts", args.product, args.target,
                                               *before["packageLocks"], current=built)
        closure = NATIVE["product_witness"]("library-closure", args.target, host, current=built)
        import_results = imports(host, roles, args.original)
        source = json.loads(subprocess.check_output(
            [tool, "reference", args.original, host, args.original_target,
             "--work", work / "original"], env=env))
        reference = source["observations"]
        library = Path(roles[ENGINE][1])
        generated_serde = candidate_serde(built, args.target, host)
        generated = observe(
            tool,
            library,
            generated_serde,
            args.target / "release/deps",
            links,
            host,
            work / "generated",
            env,
        )
        baseline = standalone["scenarios"]["baseline"]
        assert (
            reference
            == generated
            == {key: baseline[key] for key in ("values", "traits")}
        )
        assert closure == NATIVE["product_witness"]("library-closure", args.target, host, current=built)
    assert before == snapshots(args)
    assert original == NATIVE["product_witness"]("import-source-pin", args.original)
    subprocess.run([tool, "checkout", args.framework, FRAMEWORK], check=True)
    subprocess.run([tool, "checkout", ROOT, companion], check=True)
    return {
        "platform": host,
        "rustc": rustc,
        "candidate": {"release": CANDIDATE.stem, "input_revision": INPUT_REVISION,
                      "framework_revision": FRAMEWORK, "lock_sha256": LOCK_SHA256},
        "original": original,
        "compilerSha256": digest(args.compiler),
        "inputs": before,
        "cargoArtifactLogSha256": hashlib.sha256(built.encode()).hexdigest(),
        "cargoStaticAndMetadataClosureSha256": hashlib.sha256(
            json.dumps(closure, sort_keys=True).encode()
        ).hexdigest(),
        "cargoStaticAndMetadataClosureEntries": len(closure),
        "imports": import_results,
        "engine": {
            "observations": generated,
            "originalLibrarySha256": source["originalLibrarySha256"],
            "candidateLibrarySha256": digest(library),
            "originalSerdeWitnessSha256": source["originalSerdeWitnessSha256"],
            "candidateSerdeWitnessSha256": digest(generated_serde),
        },
        "standaloneReceiptSha256": digest(ENGINE_STANDALONE / f"{host}.json"),
        "verifierSha256": digest(Path(__file__)),
        "helperSha256": HELPER_HASHES,
        "archivedComposer": {"path": ARCHIVED_COMPOSER, "revision": INPUT_REVISION, "sha256": ARCHIVED_COMPOSER_SHA},
        "nativeComposer": {
            "companion": companion, "executableSha256": digest(NATIVE["workspace_tool"]()[1]),
            "sourceSha256": {name: digest(WHOLE / name) for name in (
                "canonical-range-data/compose.rs", "converged-public-api/compose.rs",
                "canonical-public-data/src/lib.rs", "canonical-public-data/src/assembly.rs",
                "canonical-public-data/witness.rs", "canonical-public-data/Cargo.toml",
                "canonical-public-data/Cargo.lock", "unified-mdk/workspace_compat.py",
            )},
        },
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for parameter in (
        "framework",
        "compiler",
        "original",
        "original_target",
        "baseline",
        "product",
        "target",
        "receipt",
    ):
        parser.add_argument(parameter, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    receipt = qualify(args)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        {
            key: receipt["imports"][key]
            for key in ("attempted", "baseline", "candidate", "added")
        }
    )
