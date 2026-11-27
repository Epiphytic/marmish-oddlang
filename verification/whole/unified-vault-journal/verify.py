"""Qualify the three public contracts against one sealed native release build."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
PREVIOUS = PORTABLE / "workspace-v7-recovered-vault-engine.json"
MANIFEST = PORTABLE / "workspace-v7-recovered-vault-engine-journal.json"
FRAMEWORK = "84b173b288ef41b60c87aa45291f3f20ccb44bed"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
COMPOSED_SOURCE = "e54db134a2cf649c5cffcf3822f4e98b3439f141"
JOURNAL_SOURCE = "380043bf9e7686e24cc00436486396ef4308edfb"
HOST = "aarch64-apple-darwin"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def data(path):
    return json.loads(path.read_bytes())


def lane(name, script="verify.py"):
    return runpy.run_path(str(HERE.parent / name / script))


JOURNAL = lane("journal-records")
run = JOURNAL["run"]


def require_pin(path, identity):
    if (run("git", "rev-parse", "HEAD", cwd=path).decode().strip() != identity
        or run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)):
        raise ValueError(f"expected clean checkout {identity}: {path}")


def check_input_composition():
    old, new = data(PREVIOUS), data(MANIFEST)
    old_native = PORTABLE / old["members"]["marmish-native"]["manifest"]["path"]
    new_native = PORTABLE / new["members"]["marmish-native"]["manifest"]["path"]
    desired = data(old_native)
    journal, = data(PORTABLE / "native-journal-records-package.json")["components"]
    desired["components"].append(journal)
    if desired != data(new_native):
        raise ValueError("combined native input differs beyond one public record component")
    old["members"]["marmish-native"]["manifest"] = new["members"]["marmish-native"]["manifest"]
    if old != new:
        raise ValueError("combined workspace changed its Cargo pin, siblings or edges")


def check_product(workspace, lock):
    frozen = MANIFEST.with_suffix("") / "workspace.lock"
    if lock.read_bytes() != frozen.read_bytes():
        raise ValueError("the assembled workspace disagrees with its sealed lock")
    old = data(PREVIOUS.with_suffix("") / "workspace.lock")
    current = data(lock)
    for key in ("workspaceCargoManifestSha256", "cargoLockSha256"):
        if current[key] != old[key]:
            raise ValueError(f"combined Cargo workspace identity changed: {key}")
    if current["members"].keys() != old["members"].keys():
        raise ValueError("combined workspace member inventory changed")
    for name, known in old["members"].items():
        package = workspace / name
        observed = current["members"][name]
        if name == "marmish-native":
            for key in ("dependencies", "workspaceCargoManifestSha256"):
                if observed[key] != known[key]:
                    raise ValueError(f"combined native package changed {key}")
        elif observed != known:
            raise ValueError(f"combined workspace changed member {name}")
        package_lock = package / "package.lock"
        if sha(package_lock) != observed["standalonePackageLockSha256"]:
            raise ValueError(f"actual package lock differs for {name}")
        if sha(package / "Cargo.toml") != observed["workspaceCargoManifestSha256"]:
            raise ValueError(f"actual Cargo manifest differs for {name}")
        source = {p.relative_to(package).as_posix(): sha(p)
                  for p in (package / "src").rglob("*") if p.is_file()}
        if (package / "build.rs").is_file():
            source["build.rs"] = sha(package / "build.rs")
        if source != data(package_lock)["generatedFiles"]:
            raise ValueError(f"projected source inventory differs for {name}")
    if sha(workspace / "Cargo.toml") != current["workspaceCargoManifestSha256"]:
        raise ValueError("assembled root Cargo manifest differs")
    if sha(workspace / "Cargo.lock") != current["cargoLockSha256"]:
        raise ValueError("assembled root Cargo lock differs")
    return current


def source_oracles(compiler, original, scratch, prior, work):
    inputs = work / "inputs"
    with patch.dict(prior["check_recovered_report"].__globals__, {"MANIFEST": MANIFEST}):
        release = prior["check_recovered_report"](inputs)
    recovered = scratch / "original-report"
    run("cargo", "+1.97.1", "run", "--offline", "--locked", "--quiet",
        "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
        "--bin", "recover-release-info", "--", compiler, original, recovered,
        scratch / "report-selection.json")
    candidate = data(recovered / "graph.json")
    candidate["guards"] = data(PORTABLE / "native-build-report-v1/graph.json")["guards"]
    if candidate != data(inputs / "native-build-report-recovered-v1/graph.json"):
        raise ValueError("source-recovered report graph changed")
    mutant = scratch / "journal-mutant"
    JOURNAL["mutate"](original, mutant)
    changed = JOURNAL["recover"](compiler, original, mutant, scratch)
    previous = data(HERE.parent / "journal-records/aarch64-apple-darwin.json")
    if data(changed / "recovery.lock") != previous["reorderedRecovery"]:
        raise ValueError("journal source mutation no longer recovers the qualified graph")
    return release, {"baselineRecoveryLockSha256": sha(scratch / "recovered/recovery.lock"),
                     "reorderedRecoveryLockSha256": sha(changed / "recovery.lock")}


def qualify(work, scratch, args):
    assembly = lane("workspace")
    prior = lane("unified-recovered-vault")
    compiler_target = work / "compiler"
    run("cargo", "+1.97.1", "build", "--offline", "--locked", "--bin", "oddlang",
        "--target-dir", compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
    compiler = compiler_target / "debug/oddlang"
    inputs = work / "inputs"
    inputs.mkdir()
    spec = assembly["stage_workspace"](MANIFEST, inputs)
    staged = {p.relative_to(inputs).as_posix(): sha(p)
              for p in sorted(inputs.rglob("*")) if p.is_file()}
    if any(Path(name).suffix not in {".json", ".ttl", ".lock"} for name in staged):
        raise ValueError("unexpected non-portable workspace input")
    cache = work / "providers"
    sources = assembly["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)

    prefix, env = JOURNAL["sandbox"](assembly, work, (
        ROOT, args.original, args.framework, args.original_target, args.original_bin_dir, scratch,
    ), cache)
    denied = {
        "original-journal": args.original / "crates/marmish-native/src/journal.rs",
        "original-vault": args.original / "crates/marmish-vault/src/lib.rs",
        "original-report": args.original / "crates/marmish-native/src/native_manifest.rs",
        "reference-executable": args.original_bin_dir / "marmish",
        "framework-source": args.framework / "src/lib.rs",
        "journal-caller": HERE.parent / "journal-records/probe.rs",
        "vault-holdout": HERE.parent / "unified-recovered-vault/fixed_bytes.rs",
        "release-observer": HERE.parent / "unified-mdk/release.rs",
    }
    for name, path in denied.items():
        if not path.is_file():
            raise ValueError(f"missing source-denial probe: {name}")
        probe = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if not probe.returncode or b"Operation not permitted" not in probe.stderr:
            raise ValueError(f"source-free gate allowed {name}")

    product = work / "product"
    run(*prefix, compiler, "build", "native-workspace", "rust",
        inputs / MANIFEST.name, product, cwd=work, env=env)
    lock = product / "workspace.lock"
    pinned = check_product(product, lock)
    build = run(*prefix, "cargo", "build", "--workspace", "--release", "--locked",
                "--offline", "--message-format=json", cwd=product, env=env)

    release_source, journal_source = source_oracles(compiler, args.original, scratch, prior, work)
    release = lane("build-report", "observe.py")["observe"](
        args.original, PORTABLE, args.original_bin_dir, product / "target/release", HOST)
    with patch.dict(prior["fixed_value_test"].__globals__, {"MANIFEST": MANIFEST}):
        fixed = prior["fixed_value_test"](work, args.original, args.original_target)

    vault = lane("vault-values")
    deps = product / "target/release/deps"
    vault_lib, = deps.glob("libmarmish_vault-*.rlib")
    vault_exe = scratch / "vault-probe"
    run("rustc", "+1.97.1", "--edition=2021", HERE.parent / "vault-values/caller.rs",
        "-L", f"dependency={deps}", "--extern", f"marmish_vault={vault_lib}", "-o", vault_exe)
    vault_output = run(vault_exe).decode().splitlines()
    vault_reference = vault["consumer"](args.original / "crates/marmish-vault", scratch / "reference-vault",
                                         args.original / "Cargo.lock")
    if vault_output != vault_reference or vault_output != vault["expected"](128):
        raise ValueError("vault public values differ on the sealed combined library")

    original_cargo = run("cargo", "+1.97.1", "build", "--release", "--offline", "--locked",
        "--lib", "-p", "marmish-native", "--message-format=json", "--target-dir",
        args.original_target, cwd=args.original, env=JOURNAL["cargo_env"]())
    original_journal = JOURNAL["probe"](scratch / "original-journal-probe", original_cargo)
    generated_journal = JOURNAL["probe"](scratch / "generated-journal-probe", build)
    if original_journal["jsonLines"] != generated_journal["jsonLines"] or generated_journal["jsonLines"] != JOURNAL["EXPECTED"]:
        raise ValueError("journal records differ on the sealed combined native library")
    return {
        "format": "marmish-unified-recovered-vault-journal-v1",
        "platform": HOST, "framework": FRAMEWORK, "original": ORIGINAL,
        "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "recoveredVaultAncestor": COMPOSED_SOURCE, "journalAncestor": JOURNAL_SOURCE,
        "compilerSha256": sha(compiler), "workspaceManifestSha256": sha(MANIFEST),
        "workspaceLockSha256": sha(lock), "cargoLockSha256": pinned["cargoLockSha256"],
        "unchangedMemberLocks": ["marmish-core", "marmish-engine", "marmish-stream", "marmish-vault"],
        "stagedInputs": staged, "deniedSourceProbes": sorted(denied), "gitProviders": sources,
        "releaseInfo": release, "recoveredReport": release_source,
        "fixedVault": fixed | {"librarySha256": sha(vault_lib), "publicValueRows": len(vault_output)},
        "journal": journal_source | {"original": original_journal, "generated": generated_journal},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("--original-target", "--original-bin-dir"):
        parser.add_argument(name, required=True, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    require_pin(args.framework, FRAMEWORK)
    require_pin(args.original, ORIGINAL)
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT):
        raise ValueError("expected clean companion checkout")
    if f"host: {HOST}" not in run("rustc", "+1.97.1", "-vV").decode().splitlines():
        raise ValueError("combined release gate requires native macOS ARM64")
    check_input_composition()
    with (tempfile.TemporaryDirectory() as work, tempfile.TemporaryDirectory() as scratch):
        result = qualify(Path(work).resolve(), Path(scratch).resolve(), args)
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
