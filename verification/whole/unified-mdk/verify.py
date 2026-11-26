"""Run all composed pure-data oracles against one sealed six-member workspace."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
from unittest.mock import patch

from compose import BASE, MEMBER, WORKSPACE, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
STANDALONE_MDK = "27b0e5e7b16629cc4ec2973b729cf45e2d9c114b"
PLATFORMS = {"aarch64-apple-darwin", "aarch64-unknown-linux-gnu"}


def lane(name, script="verify.py"):
    return runpy.run_path(str(HERE.parent / name / script))


JOURNAL = lane("journal-records")
run = JOURNAL["run"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def data(path):
    return json.loads(path.read_bytes())


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (MANIFEST.with_suffix("") / "workspace.lock").read_bytes():
        raise ValueError("six-member projection lock mismatch")
    current = data(lock)
    old = data((PORTABLE / BASE).with_suffix("") / "workspace.lock")
    members = dict(current["members"])
    added = members.pop(MEMBER)
    if members != old["members"] or len(members) != 5 or added["dependencies"]:
        raise ValueError("predecessor member locks, edges or isolated MDK member changed")
    check_member_files(product, current)
    standalone = data(HERE.parent / "mdk-public-data/darwin-arm64/receipt.json")
    if added["standalonePackageLockSha256"] != standalone["lanes"]["baseline"]["packageLockSha256"]:
        raise ValueError("MDK member differs from its standalone qualified projection")
    return current


def check_member_files(product, current):
    if sha(product / "Cargo.toml") != current["workspaceCargoManifestSha256"]:
        raise ValueError("workspace root manifest changed")
    if sha(product / "Cargo.lock") != current["cargoLockSha256"]:
        raise ValueError("workspace root Cargo lock changed")
    for name, declaration in current["members"].items():
        member = product / name
        lock = member / "package.lock"
        if sha(member / "Cargo.toml") != declaration["workspaceCargoManifestSha256"]:
            raise ValueError(f"sealed member manifest differs: {name}")
        if sha(lock) != declaration["standalonePackageLockSha256"]:
            raise ValueError(f"sealed package lock differs: {name}")
        files = {p.relative_to(member).as_posix(): sha(p)
                 for p in (member / "src").rglob("*") if p.is_file()}
        if (member / "build.rs").is_file():
            files["build.rs"] = sha(member / "build.rs")
        if files != data(lock)["generatedFiles"]:
            raise ValueError(f"sealed generated source differs: {name}")

def artifact(build, name):
    matches = []
    for line in build.splitlines():
        row = json.loads(line)
        if row["reason"] == "compiler-artifact" and row["target"]["name"] == name:
            matches.extend(Path(value) for value in row["filenames"] if value.endswith(".rlib"))
    if len(matches) != 1:
        raise ValueError(f"ambiguous native artifact {name}: {matches}")
    return matches[0]


def cargo_reference(args, name):
    return run("cargo", "+1.97.1", "build", "--release", "--offline", "--locked",
               "--lib", "-p", name, "--message-format=json", "--target-dir",
               args.original_target, cwd=args.original, env=JOURNAL["cargo_env"]())


def mdk_oracle(args, compiler, work, scratch, generated_build):
    mdk = lane("mdk-public-data")
    inputs = scratch / "mdk-source"
    inputs.mkdir()
    facts = mdk["recover"](compiler, args.original / mdk["SOURCE"], scratch, inputs)
    baseline = data(HERE.parent / "mdk-public-data/darwin-arm64/receipt.json")["lanes"]["baseline"]
    if facts != baseline["recovery"]:
        raise ValueError("MDK source recovery drifted from its standalone binding")
    for path in inputs.iterdir():
        if path.read_bytes() != (work / "inputs" / mdk["INPUTS"] / path.name).read_bytes():
            raise ValueError(f"sealed MDK member drift: {path.name}")
    upstream = cargo_reference(args, MEMBER)
    source_lib = artifact(upstream, "marmish_mdk")
    serde = artifact(upstream, "serde")
    linked = artifact(generated_build, "marmish_mdk")
    source_observed = mdk["observe"](source_lib, serde.parent, serde, serde.parent, scratch / "mdk-original")
    deps = work / "product/target/release/deps"
    generated = mdk["observe"](linked, deps, serde, serde.parent, scratch / "mdk-generated")
    if source_observed != generated:
        raise ValueError("the sealed MDK member differs from the original pure public API")
    output, negatives = generated
    if hashlib.sha256(output).hexdigest() != baseline["outputSha256"] or negatives != baseline["negativeRustDiagnostics"]:
        raise ValueError("MDK baseline differs from its independent standalone receipt")
    return {
        "scope": ["marmish_mdk::AdapterError", "marmish_mdk::VersionObservation"],
        "standaloneCompanion": STANDALONE_MDK,
        "originalLibrarySha256": sha(source_lib), "sealedLibrarySha256": sha(linked),
        "recovery": facts, "output": output.decode(), "negativeRustDiagnostics": negatives,
    }


def qualified_oracles(args, host, work, scratch, compiler, product, build, manifest=MANIFEST):
    predecessor = lane("unified-vault-journal")
    report = lane("unified-recovered-vault")

    def host_run(*argv, **options):
        if "--target" in argv:
            index = argv.index("--target") + 1
            if argv[index] == "aarch64-apple-darwin":
                argv = (*argv[:index], host, *argv[index + 1:])
        return run(*argv, **options)

    with patch.dict(predecessor["source_oracles"].__globals__, {"MANIFEST": manifest}):
        recovered, journal_source = predecessor["source_oracles"](compiler, args.original, scratch, report, work)
    release = lane("build-report", "observe.py")["observe"](
        args.original, PORTABLE, args.original_bin_dir, product / "target/release", host)
    with patch.dict(report["fixed_value_test"].__globals__, {"MANIFEST": manifest, "run": host_run}):
        fixed = report["fixed_value_test"](work, args.original, args.original_target)

    vault = lane("vault-values")
    vault_lib = artifact(build, "marmish_vault")
    probe = scratch / "vault-public-values"
    deps = product / "target/release/deps"
    run("rustc", "+1.97.1", "--edition=2021", HERE.parent / "vault-values/caller.rs",
        "-L", f"dependency={deps}", "--extern", f"marmish_vault={vault_lib}", "-o", probe)
    original_values = vault["consumer"](args.original / "crates/marmish-vault", scratch / "vault-original",
                                         args.original / "Cargo.lock")
    rows = run(probe).decode().splitlines()
    if rows != original_values or rows != vault["expected"](128):
        raise ValueError("sealed vault public values differ")
    original_journal = JOURNAL["probe"](scratch / "journal-original", cargo_reference(args, "marmish-native"))
    generated_journal = JOURNAL["probe"](scratch / "journal-generated", build)
    if original_journal["jsonLines"] != generated_journal["jsonLines"] or generated_journal["jsonLines"] != JOURNAL["EXPECTED"]:
        raise ValueError("sealed native journal public records differ")
    return {
        "recoveredReport": recovered, "releaseInfo": release,
        "fixedVault": fixed | {"librarySha256": sha(vault_lib), "publicValueRows": len(rows)},
        "journal": journal_source | {"original": original_journal, "generated": generated_journal},
        "mdk": mdk_oracle(args, compiler, work, scratch, build),
    }


def qualify(args, host, work, scratch, *, manifest=MANIFEST, base=BASE, member=MEMBER,
            check=check_product, extra_denied=None, extra_oracle=None, extra_artifact_oracle=None,
            receipt_format="marmish-six-member-mdk-public-data-v1"):
    assembly = lane("workspace")
    run("cargo", "+1.97.1", "build", "--offline", "--locked", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=JOURNAL["cargo_env"]())
    compiler = work / "oddlang"
    shutil.copy2(args.compiler_target / "debug/oddlang", compiler)
    inputs = work / "inputs"
    inputs.mkdir()
    spec = assembly["stage_workspace"](manifest, inputs)
    staged = {p.relative_to(inputs).as_posix(): sha(p)
              for p in sorted(inputs.rglob("*")) if p.is_file()}
    if any(Path(name).suffix not in {".json", ".ttl", ".lock"} for name in staged):
        raise ValueError("unexpected non-portable workspace input")
    cache = work / "providers"
    providers = assembly["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
    prefix, env = JOURNAL["sandbox"](assembly, work, (
        ROOT, args.original, args.framework, args.original_target, args.original_bin_dir,
        args.compiler_target, scratch,
    ), cache)

    def invoke(*argv, cwd=work):
        command = list(prefix)
        if sys.platform == "linux":
            command[command.index("--chdir") + 1] = str(cwd)
        return run(*command, *argv, cwd=cwd, env=env)

    denied = {
        "original-journal": args.original / "crates/marmish-native/src/journal.rs",
        "original-vault": args.original / "crates/marmish-vault/src/lib.rs",
        "original-report": args.original / "crates/marmish-native/src/native_manifest.rs",
        "original-mdk": args.original / "crates/marmish-mdk/src/lib.rs",
        "reference-executable": args.original_bin_dir / "marmish",
        "framework-source": args.framework / "src/lib.rs",
        "journal-caller": HERE.parent / "journal-records/probe.rs",
        "mdk-caller": HERE.parent / "mdk-public-data/probe.rs",
        "vault-holdout": HERE.parent / "unified-recovered-vault/fixed_bytes.rs",
        "release-observer": HERE.parent / "build-report/observe.py",
    }
    denied.update(extra_denied or {})
    for name, path in denied.items():
        if not path.is_file():
            raise ValueError(f"missing source-denial probe: {name}")
        status = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if status.returncode != 1 or status.stdout or not any(
            reason in status.stderr for reason in (b"Operation not permitted", b"No such file or directory")
        ):
            raise ValueError(f"source-free gate did not deny {name}")
    product = work / "product"
    invoke(compiler, "build", "native-workspace", "rust", inputs / manifest.name, product)
    lock = check(product)
    build = invoke("cargo", "build", "--workspace", "--release", "--locked", "--offline",
                   "--message-format=json", cwd=product)
    observations = qualified_oracles(args, host, work, scratch, compiler, product, build, manifest)
    if extra_oracle is not None:
        observations.update(extra_oracle(product))
    if extra_artifact_oracle is not None:
        observations.update(extra_artifact_oracle(product, build))
    return {
        "format": receipt_format, "platform": host,
        "framework": args.framework_revision, "original": ORIGINAL,
        "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "predecessorWorkspaceLockSha256": sha((PORTABLE / base).with_suffix("") / "workspace.lock"),
        "compilerSha256": sha(compiler), "workspaceManifestSha256": sha(manifest),
        "workspaceLockSha256": sha(product / "workspace.lock"), "cargoLockSha256": lock["cargoLockSha256"],
        "physicalMembers": sorted(lock["members"]),
        "unchangedMemberLocks": sorted(name for name in lock["members"] if name != member),
        "stagedInputs": staged, "deniedSourceProbes": sorted(denied), "gitProviders": providers,
    } | observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda path: Path(path).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for path, revision in ((args.framework, args.framework_revision), (args.original, ORIGINAL)):
        if run("git", "rev-parse", "HEAD", cwd=path).decode().strip() != revision or run(
            "git", "status", "--porcelain", "--untracked-files=all", cwd=path
        ):
            raise ValueError(f"expected clean checkout {revision}: {path}")
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT):
        raise ValueError("expected a clean companion checkout")
    host, = (line.removeprefix("host: ") for line in run("rustc", "+1.97.1", "-vV").decode().splitlines()
             if line.startswith("host: "))
    if host not in PLATFORMS:
        raise ValueError(f"unsupported native qualification host {host}")
    compose(PORTABLE, check=True)
    with tempfile.TemporaryDirectory(prefix="odd-six-data-") as work:
        with tempfile.TemporaryDirectory(prefix="odd-six-oracles-") as scratch:
            receipt = qualify(args, host, Path(work).resolve(), Path(scratch).resolve())
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
