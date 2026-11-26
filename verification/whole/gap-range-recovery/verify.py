"""Recover the native gap record and replay the archived original data witnesses."""

import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
FRAMEWORK = "9923e6664899de66915ed3a94e417f261ffed401"
LEGACY = "77c4c6272bf317e11cf9a2783eb7b00e3fcca0f0"
REASON = "cf2cd3940b790740d6e090bdcbf8702681c656e4"
AUTHORED = PORTABLE / "engine-gap-range-v1"
RECORD = "engine-gap-range-recovered-v1"
PACKAGE = "engine-gap-range-recovered-package.json"
REASON_GRAPH = "engine-gap-reason-recovered-v1"
ID = "urn:marmish:engine:output-gap:"
BUILD_RUSTC = "31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd"
RUSTC = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HOSTS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")


def pinned(path, commit):
    recorded = subprocess.check_output(
        ["git", "show", f"{commit}:{path.relative_to(ROOT)}"], cwd=ROOT
    )
    if path.read_bytes() != recorded:
        raise ValueError(f"pinned verification input differs: {path.name}")
    return recorded


prior_helper = HERE.parent / "gap-range-record/verify.py"
pinned(prior_helper, LEGACY)
PRIOR = runpy.run_path(str(prior_helper))
run, sha, dump, clean = (PRIOR[key] for key in ("run", "sha", "dump", "clean"))
PIN, MODEL, ENV, BASELINE = (PRIOR[key] for key in ("PIN", "MODEL", "ENV", "BASELINE"))


def read(path):
    return json.loads(path.read_bytes())


def selection(source, rustdoc):
    document = read(rustdoc)
    (filename,) = {
        item["span"]["filename"]
        for item in document["index"].values()
        if item["name"] == "MissingRange"
        and "struct" in item["inner"]
        and item["crate_id"] == 0
        and item["span"] is not None
    }
    if filename != MODEL.as_posix():
        raise ValueError("unexpected compiled declaration source path")
    return {
        "format": "oddlang-serde-public-record-selection-v1",
        "sourceFilename": filename,
        "sourceSha256": sha(source),
        "rustdocSha256": sha(rustdoc),
        "crateName": "marmish_engine",
        "crateRootSha256": sha(source.with_name("lib.rs")),
        "moduleId": ID + "range-module",
        "moduleName": "model",
        "record": {
            "name": "MissingRange",
            "id": ID + "range",
            "fields": {
                name: ID + "range:" + name for name in ("start", "end", "reason")
            },
        },
        "choices": [{"name": "MissingReason", "id": ID + "reason"}],
    }


def rustdoc_inputs(directory, original, sources, host):
    evidence = read(directory / "evidence.json")
    profile = {
        "format": "marmish-linked-range-rustdoc-v1",
        "original": PIN,
        "host": host,
        "source": MODEL.as_posix(),
        "cargoLockSha256": sha(original / "Cargo.lock"),
        "runnerSha256": sha(HERE / "rustdoc.py"),
        "crateRootSha256": sha((original / MODEL).with_name("lib.rs")),
        "helperSha256": sha(prior_helper),
        "cargoArgs": [
            "cargo",
            "+1.97.1",
            "rustdoc",
            "--locked",
            "--offline",
            "--lib",
            "-p",
            "marmish-engine",
            "--target",
            host,
            "--target-dir",
            "<target-dir>",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
            "--document-private-items",
        ],
        "profileEnvironment": {
            "RUSTC_BOOTSTRAP": "1",
            "CARGO_PROFILE_DEV_DEBUG": "0",
            "CARGO_INCREMENTAL": "0",
        },
    }
    if any(evidence[key] != value for key, value in profile.items()):
        raise ValueError("rustdoc runner, original source or profile differs")
    if (
        f"commit-hash: {RUSTC}" not in evidence["rustdoc"]
        or f"host: {host}" not in evidence["rustdoc"]
    ):
        raise ValueError("wrong native rustdoc toolchain")
    inputs = {}
    for label, source in sources.items():
        document = directory / f"{label}.json"
        compiled = read(document)
        if (
            compiled["format_version"] != 57
            or not compiled["includes_private"]
            or compiled["target"]["triple"] != host
            or compiled["index"][str(compiled["root"])]["name"] != "marmish_engine"
        ):
            raise ValueError(f"wrong compiled rustdoc document for {label}")
        wanted = {
            "sourceSha256": sha(source),
            "rustdocSha256": sha(document),
            "rustdocFile": document.name,
        }
        if evidence["inputs"][label] != wanted:
            raise ValueError(f"wrong compiler evidence inputs for {label}")
        inputs[label] = document
    if sha(inputs["baseline"]) == sha(inputs["mutation"]):
        raise ValueError("the two independent rustdoc builds must differ")
    return inputs, evidence


def recover(compiler, source, doc, work):
    picked, out = work / "selection.json", work / "recovered"
    work.mkdir()
    dump(picked, selection(source, doc))
    run(compiler, "recover", "serde-public-record", source, doc, picked, out, env=ENV)
    lock = read(out / "recovery.lock")
    pins = {
        "selection": picked,
        "source": source,
        "crateRoot": source.with_name("lib.rs"),
        "rustdoc": doc,
        "graph": out / "graph.json",
        "overlay": out / "rust.json",
    }
    if lock["format"] != "oddlang-serde-public-record-recovery-lock-v1" or any(
        lock[key + "Sha256"] != sha(path) for key, path in pins.items()
    ):
        raise ValueError(
            "compiler recovery lock differs from the selected inputs or outputs"
        )
    return picked, out


def qualification_witnesses(original, host, sources, rustc):
    receipt = HERE.parent / "gap-range-record" / f"{host}.json"
    archived = json.loads(pinned(receipt, LEGACY))
    legacy_profile = {
        "original": PIN,
        "format": "marmish-public-gap-range-v1",
        "rustc": rustc,
        "sourceModelSha256": sha(sources["baseline"]),
        "changedRows": ["unknown-field"],
    }
    if any(archived[key] != value for key, value in legacy_profile.items()):
        raise ValueError("archived native qualification profile differs")
    baseline, baseline_sha = PRIOR["fixture_reference"](original, host)
    if (
        archived["originalReferenceSha256"] != baseline_sha
        or archived["baseline"]["observations"] != baseline
    ):
        raise ValueError("archived native baseline differs")
    if archived["mutationSourceModelSha256"] != sha(sources["mutation"]):
        raise ValueError(
            "mutation source differs from the independently compiled original"
        )
    if archived["unchangedProbeSha256"] != sha(BASELINE / "probe.rs") or archived[
        "casesSha256"
    ] != sha(BASELINE / "cases.json"):
        raise ValueError("caller or corpus changed")
    ids = [row["id"] for row in read(BASELINE / "cases.json")]
    mutation = archived["mutation"]["observations"]
    if len(set(ids)) != 35 or any(
        [row["id"] for row in rows] != ids for rows in (baseline, mutation)
    ):
        raise ValueError("wrong native case inventory")
    changed = [a["id"] for a, b in zip(baseline, mutation, strict=True) if a != b]
    if changed != ["unknown-field"] or tuple(
        PRIOR["observed_policy"](rows) for rows in (baseline, mutation)
    ) != ("ignore", "reject"):
        raise ValueError("native oracle approved a different mutation outcome")
    if archived["nativeTraitsProbeSha256"] != sha(prior_helper.with_name("traits.rs")):
        raise ValueError("archived traits witness changed")
    return {"baseline": baseline, "mutation": mutation}, sha(receipt), baseline_sha


def reason_witness(host, baseline_source):
    path = HERE.parent / "gap-reason-recovery" / f"{host}.json"
    archived = json.loads(pinned(path, REASON))
    if (archived["original"], archived["host"], archived["sourceSha256"]) != (
        PIN,
        host,
        sha(baseline_source),
    ):
        raise ValueError("the linked choice's source provenance differs")
    recorded = archived["baselineRecovery"]
    for key, name in (("graph", "graph.json"), ("overlay", "rust.json")):
        if recorded[key + "Sha256"] != sha(PORTABLE / REASON_GRAPH / name):
            raise ValueError(
                "the linked choice did not come from the archived recovery"
            )
    return {"framework": archived["framework"], "receiptSha256": sha(path)}


def stage(recovered, inputs):
    authored_path = PORTABLE / "engine-gap-range-package.json"
    manifest = json.loads(pinned(authored_path, LEGACY))
    for name, component in (("graph.json", "graph"), ("rust.json", "overlay")):
        reason = PORTABLE / REASON_GRAPH / name
        pinned(reason, REASON)
        target = inputs / REASON_GRAPH / name
        target.parent.mkdir(exist_ok=True, parents=True)
        shutil.copyfile(reason, target)
        manifest["components"][0][component] = PRIOR["pin"](inputs, target)

        record = inputs / RECORD / name
        record.parent.mkdir(exist_ok=True, parents=True)
        shutil.copyfile(recovered / name, record)
        manifest["components"][1][component] = PRIOR["pin"](inputs, record)
    cargo = PRIOR["checked_pin"](PORTABLE, manifest["cargoLock"])
    lock = inputs / manifest["cargoLock"]["path"]
    lock.parent.mkdir(exist_ok=True, parents=True)
    shutil.copyfile(cargo, lock)
    path = inputs / PACKAGE
    dump(path, manifest)
    return path


def sealed(
    compiler, original, framework, scratch, inputs, recovered, expected, host, work
):
    tool = Path(shutil.copy2(compiler, work / "oddlang"))
    forbidden = [
        original / MODEL,
        (original / MODEL).with_name("lib.rs"),
        scratch / "original-mutant" / MODEL,
        (scratch / "original-mutant" / MODEL).with_name("lib.rs"),
        *inputs.values(),
        BASELINE / "probe.rs",
        prior_helper.with_name("traits.rs"),
    ]
    prefix, env = PRIOR["JOURNAL"]["sandbox"](
        PRIOR["ASSEMBLY"],
        work,
        (ROOT, original, framework, compiler.parent, scratch, *forbidden),
        None,
    )
    for path in forbidden:
        deny = subprocess.run(
            [*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True
        )
        if (
            not path.is_file()
            or deny.returncode != 1
            or deny.stdout
            or not any(
                marker in deny.stderr
                for marker in (b"No such file", b"Operation not permitted")
            )
        ):
            raise ValueError(f"source isolation failed for {path.name}")

    def inside(*argv, cwd, **options):
        wrapper = list(prefix)
        if host.endswith("linux-gnu"):
            wrapper[wrapper.index("--chdir") + 1] = str(cwd)
        return run(*wrapper, *argv, cwd=cwd, env=env, **options)

    products = {}
    for label, path in recovered.items():
        lane = work / label
        manifest = stage(path, lane / "inputs")
        product = lane / "product"
        inside(tool, "build", "native-package", "rust", manifest, product, cwd=lane)
        products[label] = lane, manifest, product

    receipts = {}
    for label, (lane, manifest, product) in products.items():
        path = recovered[label]
        package_lock = product / "package.lock"
        sources = read(package_lock)["generatedFiles"]
        if sources != {str(name): sha(product / name) for name in sources}:
            raise ValueError("generated Rust bytes differ from native package lock")
        if (
            label == "baseline"
            and sources
            != json.loads(pinned(AUTHORED / "package.lock", LEGACY))["generatedFiles"]
        ):
            raise ValueError(
                "baseline recovered graph changes the generated native source"
            )
        caller = lane / "caller"
        (caller / "src").mkdir(parents=True)
        for source, name in (
            ("caller.toml", "Cargo.toml"),
            ("caller.lock", "Cargo.lock"),
            ("probe.rs", "src/main.rs"),
        ):
            shutil.copyfile(BASELINE / source, caller / name)
        inside(
            "cargo",
            "rustc",
            "--locked",
            "--offline",
            "--release",
            "--",
            "--cfg",
            "with_range",
            cwd=caller,
        )
        observed = json.loads(
            inside(
                caller / "target/release/gap-observer",
                cwd=caller,
                input=(BASELINE / "cases.json").read_bytes(),
            )
        )
        if observed != expected[label]:
            raise ValueError(
                f"recovered native data differs from the independent {label} witness"
            )
        shutil.copyfile(prior_helper.with_name("traits.rs"), caller / "src/main.rs")
        inside("cargo", "build", "--locked", "--offline", "--release", cwd=caller)
        if (
            inside(caller / "target/release/gap-observer", cwd=caller)
            != PRIOR["TRAIT_OUTPUT"]
        ):
            raise ValueError("the native public traits witness differed")
        receipts[label] = {
            "observations": observed,
            "manifestSha256": sha(manifest),
            "packageLockSha256": sha(package_lock),
            "generatedFiles": sources,
        }
        shutil.copyfile(manifest, path / "package.json")
        shutil.copyfile(package_lock, path / "package.lock")
    return receipts, len(forbidden)


def record_policy(graph):
    (record,) = graph["modules"][0]["records"]
    return record["readPolicy"].pop("unknownFields")


def qualify(args):
    if args.framework_revision != FRAMEWORK:
        raise ValueError(f"expected the qualified recovery runtime {FRAMEWORK}")
    companion = run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip()
    clean(ROOT, companion)
    clean(args.framework, args.framework_revision)
    clean(args.original, PIN)
    if any(
        path.is_relative_to(tree)
        for path in (args.compiler_target, args.output)
        for tree in (ROOT, args.original, args.framework)
    ):
        raise ValueError(
            "place the compiler target and evidence output outside the source trees"
        )
    rustc = run("rustc", "+1.97.1", "-vV", env=ENV).decode().splitlines()
    (host,) = [line[6:] for line in rustc if line.startswith("host: ")]
    expected = {"Darwin": "arm64", "Linux": "aarch64"}
    if (
        host not in HOSTS
        or f"commit-hash: {RUSTC}" not in rustc
        or os.uname().machine != expected.get(os.uname().sysname)
    ):
        raise ValueError("expected pinned native ARM64 compiler")
    builder = run("rustc", "+1.96.1", "-vV", env=ENV).decode().splitlines()
    if f"commit-hash: {BUILD_RUSTC}" not in builder or f"host: {host}" not in builder:
        raise ValueError("expected pinned native compiler build toolchain")
    run(
        "cargo",
        "+1.96.1",
        "build",
        "--locked",
        "--offline",
        "--bin",
        "oddlang",
        "--target-dir",
        args.compiler_target,
        cwd=args.framework,
        env=ENV,
    )
    compiler = args.compiler_target / "debug/oddlang"
    with (
        tempfile.TemporaryDirectory(prefix="linked-range-source-") as area,
        tempfile.TemporaryDirectory(prefix="linked-range-sealed-") as isolated,
    ):
        scratch, work = Path(area).resolve(), Path(isolated).resolve()
        mutant = PRIOR["mutate_original"](args.original, scratch)
        sources = {"baseline": args.original / MODEL, "mutation": mutant / MODEL}
        crate_root = sha(sources["baseline"].with_name("lib.rs"))
        if sha(sources["mutation"].with_name("lib.rs")) != crate_root:
            raise ValueError("the source mutation changed the crate root")
        docs, evidence = rustdoc_inputs(args.rustdoc, args.original, sources, host)
        reason = reason_witness(host, sources["baseline"])
        expected, legacy_sha, baseline_sha = qualification_witnesses(
            args.original, host, sources, rustc
        )
        recovered, selections = {}, {}
        for label, source in sources.items():
            selections[label], recovered[label] = recover(
                compiler, source, docs[label], scratch / label
            )
        denied = scratch / "stale-selector-output"
        rejected = subprocess.run(
            [
                compiler,
                "recover",
                "serde-public-record",
                sources["mutation"],
                docs["mutation"],
                selections["baseline"],
                denied,
            ],
            env=ENV,
            capture_output=True,
        )
        if (
            not rejected.returncode
            or denied.exists()
            or b"digest" not in rejected.stderr
        ):
            raise ValueError("a stale source/rustdoc selector was accepted")

        baseline, mutation = (read(recovered[key] / "graph.json") for key in sources)
        if baseline != json.loads(pinned(AUTHORED / "graph.json", LEGACY)):
            raise ValueError(
                "recovered baseline graph differs from the authored contract"
            )
        overlays = [read(recovered[key] / "rust.json") for key in sources]
        if any(
            overlay != json.loads(pinned(AUTHORED / "rust.json", LEGACY))
            for overlay in overlays
        ):
            raise ValueError("the recovered Rust overlay changed")
        policies = record_policy(baseline), record_policy(mutation)
        if policies != ("ignore", "reject") or baseline != mutation:
            raise ValueError(
                "the source mutation must change only the unknown-field policy"
            )

        observations, denials = sealed(
            compiler,
            args.original,
            args.framework,
            scratch,
            docs,
            recovered,
            expected,
            host,
            work,
        )
        if rustdoc_inputs(args.rustdoc, args.original, sources, host)[1] != evidence:
            raise ValueError("rustdoc inputs changed during qualification")
        if (
            run("git", "status", "--porcelain", "--untracked-files=all", cwd=mutant)
            != b" M crates/marmish-engine/src/model.rs\n"
        ):
            raise ValueError("the controlled mutation changed during observation")
        args.output.mkdir()
        for (label, directory), policy in zip(recovered.items(), policies, strict=True):
            target = args.output / label
            shutil.copytree(directory, target)
            shutil.copyfile(selections[label], target / "selection.json")
            observations[label].update(
                {
                    "selectionSha256": sha(target / "selection.json"),
                    "recoveryLockSha256": sha(target / "recovery.lock"),
                    "graphSha256": sha(target / "graph.json"),
                    "overlaySha256": sha(target / "rust.json"),
                    "unknownFields": policy,
                }
            )
    clean(ROOT, companion)
    clean(args.original, PIN)
    clean(args.framework, args.framework_revision)
    dump(
        args.output / "receipt.json",
        {
            "format": "marmish-linked-range-recovery-v1",
            "framework": args.framework_revision,
            "companion": companion,
            "compilerSha256": sha(compiler),
            "compilerBuildToolchain": builder,
            "linkedChoiceRecovery": reason,
            "original": PIN,
            "rustc": rustc,
            "host": host,
            "rustdocEvidence": evidence,
            "crateRootSha256": crate_root,
            "nativeAuthoredReceiptSha256": legacy_sha,
            "originalReferenceSha256": baseline_sha,
            "callerSha256": sha(BASELINE / "probe.rs"),
            "casesSha256": sha(BASELINE / "cases.json"),
            "traitsSha256": sha(prior_helper.with_name("traits.rs")),
            "verifierSha256": sha(Path(__file__)),
            "scopeSourceDenials": denials,
            "staleSelectorRejected": True,
            "changedNativeRows": ["unknown-field"],
            "cases": observations,
        },
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler_target", "original", "rustdoc", "output"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    parser.add_argument("--framework-revision", required=True)
    qualify(parser.parse_args())
