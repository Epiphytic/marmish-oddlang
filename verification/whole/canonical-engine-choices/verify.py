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
LINUX_IMPORT_PIN = "a5c9b34e04ffdc1fd4836916ffaa75df1d225f9f"
RECOVERY_PIN = "d45ac05ae29feba3379f488caacc3b18e6e38ed9"
ENGINE_STANDALONE = WHOLE / "engine-fieldless-choices"
ADDED = [("engine", "Stream"), ("engine", "SubmitDisposition")]
LOCK = CANDIDATE.with_suffix("") / "workspace.lock"


def pin_bytes(path, revision):
    want = subprocess.check_output(
        ["git", "show", f"{revision}:{path.relative_to(ROOT)}"], cwd=ROOT
    )
    if want != path.read_bytes():
        raise ValueError(f"pinned input changed: {path}")
    return want


IMPORT_TOOLING_PIN = "e434a48817f2456498ae3de689939b84d931315c"
VAULT_OBSERVER_PIN = "50b38f6113b553797e6e51871af4d2f4e5b13ba3"
WORKSPACE_TOOLING_PIN = "b0cd9f99ad03d87054420fcaca9d4964bdd9d7d6"
VAULT_MODE_PIN = "ff41a8961c5de372e15f0067705920a95eb7fb31"
HELPER_PINS = {
    ENGINE_STANDALONE / "verify.py": RECOVERY_PIN,
    WHOLE / "canonical-public-data/src/bin/engine-choice-inputs.rs": RECOVERY_PIN,
    ENGINE_STANDALONE / "probe.rs": STANDALONE_PIN,
    WHOLE / "vault-mode/verify.py": VAULT_MODE_PIN,
    WHOLE / "canonical-public-data/src/bin/vault-mode-observer.rs": VAULT_MODE_PIN,
    WHOLE / "vault-mode/observe.rs": VAULT_MODE_PIN,
    WHOLE / "vault-fixed-bytes/witness.rs": VAULT_OBSERVER_PIN,
    WHOLE / "core-package/verify.py": VAULT_OBSERVER_PIN,
    WHOLE / "workspace/verify.py": WORKSPACE_TOOLING_PIN,
    WHOLE / "canonical-linux-imports/verify.py": IMPORT_TOOLING_PIN,
    WHOLE / "seven-public-imports/verify.py": IMPORT_TOOLING_PIN,
    WHOLE / "canonical-public-data/imports.rs": IMPORT_TOOLING_PIN,
    WHOLE / "canonical-public-data/product.rs": "58b9e203c946429008ca54ebbfb16d62fc10fe8f",
    WHOLE / "canonical-public-data/src/artifacts.rs": IMPORT_TOOLING_PIN,
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
PREVIOUS = runpy.run_path(str(WHOLE / "canonical-linux-imports/verify.py"))
AUDIT = PREVIOUS["audit"]
DARWIN = AUDIT.HOST
LINUX = PREVIOUS["HOST"]


def engine_lane():
    return runpy.run_path(str(ENGINE_STANDALONE / "verify.py"))


def pinned(path, commit):
    return json.loads(pin_bytes(path, commit))


def prior(host):
    if host == DARWIN:
        receipt = pinned(PREVIOUS["DARWIN"], PREVIOUS["DARWIN_EVIDENCE"])
        assert receipt["workspaceLockSha256"] == PREVIOUS["CANONICAL"].lock_sha256
        return receipt["darwinPublicImports"]["libraries"]
    receipt = pinned(
        WHOLE / "canonical-linux-imports/aarch64-unknown-linux-gnu.json",
        LINUX_IMPORT_PIN,
    )
    assert receipt["candidate"] == PREVIOUS["CANONICAL"]._asdict()
    return receipt["generatedBuild"]["libraries"]


def snapshots(args, profile):
    NATIVE["check_api_release"]("canonical-engine-choices", PORTABLE)
    prior_input = PREVIOUS["CANONICAL"]
    AUDIT.verify_product(args.baseline, ROOT, prior_input)
    AUDIT.verify_product(args.product, ROOT, profile)
    old_workspace, workspace = map(
        AUDIT.read, [args.baseline / "workspace.lock", args.product / "workspace.lock"]
    )
    assert old_workspace.keys() == workspace.keys()
    assert all(
        workspace[k] == old_workspace[k]
        for k in workspace
        if k not in ("inputSha256", "members")
    )
    assert len(workspace["members"]) == 7
    old_engine, new_engine = (
        value["members"][ENGINE] for value in (old_workspace, workspace)
    )
    assert all(
        old_engine[key] == new_engine[key]
        for key in ("dependencies", "workspaceCargoManifestSha256")
    )
    old_packages = {
        name: AUDIT.read(args.baseline / name / "package.lock")
        for name in old_workspace["members"]
    }
    locks = {
        name: digest(args.product / name / "package.lock")
        for name in workspace["members"]
    }
    for name in workspace["members"]:
        if name == ENGINE:
            continue
        assert workspace["members"][name] == old_workspace["members"][name]
        assert locks[name] == digest(args.baseline / name / "package.lock")

    engine = args.product / ENGINE
    expected = PORTABLE / "engine-fieldless-choices-v1/package.lock"
    standalone = pinned(expected, STANDALONE_PIN)
    assert (engine / "package.lock").read_bytes() == expected.read_bytes()
    before = old_packages[ENGINE]
    assert standalone["components"][:-1] == before["components"]
    assert standalone["components"][-1]["reexports"] == ["Stream", "SubmitDisposition"]
    assert standalone["cargoLockSha256"] == before["cargoLockSha256"]
    inherited, generated = before["generatedFiles"], standalone["generatedFiles"]
    changed = {path for path in inherited if inherited[path] != generated[path]}
    added = generated.keys() - inherited.keys()
    assert changed == {"src/lib.rs", "src/group_1/mod.rs"}
    assert added == {"src/group_1/__projection_3.rs"}
    for path, line in (
        ("src/lib.rs", "pub use model::{Stream, SubmitDisposition};\n"),
        (
            "src/group_1/mod.rs",
            "mod __projection_3;\npub use __projection_3::{Stream, SubmitDisposition};\n",
        ),
    ):
        assert (engine / path).read_bytes() == (
            args.baseline / ENGINE / path
        ).read_bytes() + line.encode()
    return {
        "rootCargoLockSha256": workspace["cargoLockSha256"],
        "rootCargoManifestSha256": workspace["workspaceCargoManifestSha256"],
        "packageLocks": locks,
        "changedEngineSources": sorted(changed),
        "addedEngineSources": sorted(added),
    }


def build(args, host):
    if host == LINUX:
        return PREVIOUS["build"](args.product, args.target)
    argv = [
        "cargo",
        "+1.97.1",
        "build",
        "--workspace",
        "--lib",
        "--release",
        "--locked",
        "--offline",
        "--target-dir",
        str(args.target),
        "--message-format=json",
    ]
    env = {
        key: os.environ[key]
        for key in ("HOME", "PATH", "CARGO_HOME", "TMPDIR", "SDKROOT")
        if key in os.environ
    }
    return subprocess.check_output(
        argv, cwd=args.product, env=env | {"CARGO_NET_OFFLINE": "true", "LC_ALL": "C"}
    )


def imports(host, artifacts, original):
    previous = prior(host)
    result = AUDIT.library_imports(artifacts, original, target=host)
    additions = []
    for name, library in result.items():
        assert all(
            library[key] == previous[name][key]
            for key in (
                "inventorySha256",
                "inventoryLockSha256",
                "inventoryEvidenceSha256",
            )
        )
        for old, new in zip(previous[name]["imports"], library["imports"], strict=True):
            assert (
                old["spelling"] == new["spelling"]
                and old["originalInventoryKind"] == new["originalInventoryKind"]
            )
            if new["compiles"] and not old["compiles"]:
                assert [item["code"] for item in old["diagnostics"]] == ["E0432"]
                additions.append((name, new["spelling"]))
            else:
                assert new["compiles"] == old["compiles"], "inherited import was lost"
                assert [(d["code"], d["message"]) for d in new["diagnostics"]] == [
                    (d["code"], d["message"]) for d in old["diagnostics"]
                ], "inherited import diagnostic changed"
    attempted = sum(len(library["imports"]) for library in result.values())
    baseline = sum(library["importable"] for library in previous.values())
    current = sum(library["importable"] for library in result.values())
    assert (
        attempted == 140
        and baseline == 77
        and current == baseline + len(ADDED)
        and additions == ADDED
    )
    return {
        "attempted": attempted,
        "baseline": baseline,
        "candidate": current,
        "added": additions,
        "libraries": result,
    }


def candidate_serde(raw, target, host):
    release = target / host / "release" if host == LINUX else target / "release"
    (rlib,) = (
        Path(path).resolve()
        for line in raw.splitlines()
        if (row := json.loads(line))["reason"] == "compiler-artifact"
        and row["target"]["kind"] == ["lib"]
        and row["target"]["name"] == "serde"
        and row["profile"]["opt_level"] == "3"
        and "derive" in row["features"]
        for path in row["filenames"]
        if path.endswith(".rlib") and Path(path).is_relative_to(release)
    )
    return rlib


def qualify(args):
    assert args.input_revision == INPUT_REVISION and digest(LOCK) == LOCK_SHA256
    runtime = engine_lane()
    companion = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    runtime["checkout"](ROOT, companion)
    runtime["checkout"](args.framework, FRAMEWORK)
    original = PREVIOUS["source_pin"](args.original)
    assert all(
        not target.is_relative_to(root)
        for target in (args.target, args.original_target)
        for root in (args.product, args.baseline, args.original, args.framework, ROOT)
    )
    assert args.target != args.original_target
    rustc = AUDIT.run(["rustc", "+1.97.1", "-vV"], HERE).stdout.decode().splitlines()
    (host,) = (line[6:] for line in rustc if line.startswith("host: "))
    assert host in (DARWIN, LINUX) and os.uname().machine == (
        "arm64" if host == DARWIN else "aarch64"
    )
    assert f"commit-hash: {AUDIT.COMPILER}" in rustc
    standalone = pinned(ENGINE_STANDALONE / f"{host}.json", STANDALONE_PIN)
    assert standalone["framework"] == FRAMEWORK and standalone[
        "compilerSha256"
    ] == digest(args.compiler)
    PREVIOUS["frozen_inputs"](ROOT)
    archived_verifier = subprocess.check_output(
        ["git", "show", f"{STANDALONE_PIN}:verification/whole/engine-fieldless-choices/verify.py"], cwd=ROOT)
    assert hashlib.sha256(archived_verifier).hexdigest() == standalone["verifierSha256"]
    assert digest(ENGINE_STANDALONE / "probe.rs") == standalone["probeSha256"]
    profile = AUDIT.Candidate(CANDIDATE.stem, INPUT_REVISION, FRAMEWORK, LOCK_SHA256)
    before = snapshots(args, profile)
    with tempfile.TemporaryDirectory(
        prefix="canonical-engine-choice-observer-"
    ) as scratch:
        work = Path(scratch).resolve()
        env = runtime["SHARED"]["host_build_env"](work / "cargo-home")
        built = build(args, host)
        roles, links = AUDIT.artifacts(
            built,
            args.product,
            list(before["packageLocks"]),
            args.target,
            generated=True,
        )
        closure = PREVIOUS["closure"](built, args.target)
        import_results = imports(host, roles, args.original)
        source_library, serde, original_links = runtime["reference"](
            args.original, env, host, args.original_target
        )
        library = roles[ENGINE][1]
        reference = runtime["observe"](
            source_library,
            serde,
            args.original_target / "release/deps",
            original_links,
            host,
            work / "original",
            False,
            env,
        )
        generated_serde = candidate_serde(built, args.target, host)
        generated = runtime["observe"](
            library,
            generated_serde,
            args.target / "release/deps",
            links,
            host,
            work / "generated",
            False,
            env,
        )
        baseline = standalone["scenarios"]["baseline"]
        assert (
            reference
            == generated
            == {key: baseline[key] for key in ("values", "traits")}
        )
        assert closure == PREVIOUS["closure"](built, args.target)
    assert before == snapshots(args, profile)
    assert original == PREVIOUS["source_pin"](args.original)
    runtime["checkout"](args.framework, FRAMEWORK)
    runtime["checkout"](ROOT, companion)
    return {
        "platform": host,
        "rustc": rustc,
        "candidate": profile._asdict(),
        "original": original,
        "compilerSha256": digest(args.compiler),
        "inputs": before,
        "cargoArtifactLogSha256": AUDIT.sha(built),
        "cargoStaticAndMetadataClosureSha256": AUDIT.sha(
            json.dumps(closure, sort_keys=True).encode()
        ),
        "cargoStaticAndMetadataClosureEntries": len(closure),
        "imports": import_results,
        "engine": {
            "observations": generated,
            "originalLibrarySha256": digest(source_library),
            "candidateLibrarySha256": digest(library),
            "originalSerdeWitnessSha256": digest(serde),
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
