"""Recover a pinned record writer and compare the resulting native package with the original."""
import argparse
import importlib.util
import os
import subprocess
import shutil
from pathlib import Path
import tempfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("launch_hash_oracle", HERE.parent / "copy-launch-hash/verify.py")
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
from inputs import PIN, consumer, observe, read, run, sha, write
from verify import staged

FRAMEWORK = "aaadc7f90f1e9bab4873952811f49727490a85e6"
DATA = oracle.MANIFEST.parent / "copy-launch-layout-v1"


def expanded(source, destination):
    with tempfile.TemporaryDirectory() as target, destination.open("xb") as out:
        subprocess.run(["cargo", "+1.97.1", "rustc", "--locked", "--offline", "--lib", "-p", "marmish-stream",
                        "--target-dir", target, "--", "-Zunpretty=expanded"],
                       cwd=source, env=dict(os.environ, RUSTC_BOOTSTRAP="1"), stdout=out, check=True)
    return destination


def sealed_build(stage, cli, manifest, original, framework, *additional_sources):
    isolation_spec = importlib.util.spec_from_file_location("isolation", HERE.parent / "core-package/verify.py")
    isolation = importlib.util.module_from_spec(isolation_spec)
    isolation_spec.loader.exec_module(isolation)
    stage.mkdir()
    shutil.copyfile(cli, stage / "oddlang")
    (stage / "oddlang").chmod(0o700)
    inputs = stage / "inputs"
    shutil.copytree(manifest.parent, inputs)
    roots, env = isolation.isolation_environment(stage, "aarch64-apple-darwin")
    sources = (original, framework, oracle.ROOT, *additional_sources)
    wrapper = isolation.isolated(stage, roots, *sources)
    for repo in sources:
        isolation.run(*wrapper, "/bin/cat", repo / "Cargo.toml", cwd=stage, env=env, fail="Operation not permitted")
    generated = stage / "package"
    isolation.run(*wrapper, stage / "oddlang", "build", "native-package", "rust",
                  inputs / manifest.name, generated, cwd=stage, env=env)
    isolation.run(*wrapper, "cargo", "build", "--locked", "--release", "--offline", cwd=generated, env=env)
    return generated


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("original", "framework", "receipt"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    for repo, revision in ((original, PIN), (framework, FRAMEWORK)):
        if run("git", "rev-parse", "HEAD", cwd=repo) != revision or run("git", "status", "--porcelain", cwd=repo):
            raise ValueError(f"expected clean checkout at {revision}")
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
            "--target-dir", work / "compiler", cwd=framework)
        cli = work / "compiler/debug/oddlang"
        observed = work / "observed"
        expansion = expanded(original, work / "original-expanded.rs")
        run(cli, "recover", "record-writer", expansion,
            DATA / "selection.json", observed)
        for name in ("graph.json", "observation.json"):
            if (observed / name).read_bytes() != (DATA / "observed" / name).read_bytes():
                raise ValueError(f"changed source observation {name}")
        layout = read(observed / "graph.json")
        # Start from the released structural schema and reviewed outer operations.
        # Replace the entire digest slot list. Only the codec argument is externally supplied.
        manifest_path = staged(work, oracle.MANIFEST)
        manifest = read(manifest_path)
        pin = manifest["components"][-1]["graph"]
        contract_path = manifest_path.parent / pin["path"]
        contract = read(contract_path)
        view, = [v for v in contract["hashViews"] if v["id"] == "urn:marmish:copy:launch:view:digest"]
        if view["owner"] != layout["owner"]:
            raise ValueError("layout owner mismatch")
        if any(contract["hashTags"][key] != table for key, table in layout["externalTags"].items()):
            raise ValueError("source byte tags disagree with released conversion tables")
        view["fields"] = [{"field": s["field"], "encoding": s["encoding"]} for s in layout["fields"]]
        view["fields"].append({"parameter": "urn:marmish:copy:launch:parameter:codec", "encoding": "external-tag-u8"})
        write(contract_path, contract)
        if sha(contract_path) != pin["sha256"]:
            raise ValueError("recovered slots do not reproduce the released semantic contract")
        expansion.unlink()
        generated = sealed_build(work / "baseline-build", cli, manifest_path, original, framework)
        old = consumer(original / "crates/marmish-stream", work / "original", original / "Cargo.lock", oracle.CALLER)
        new = consumer(generated, work / "candidate", generated / "Cargo.lock", oracle.CALLER)
        requests = oracle.corpus()
        expected = [oracle.expected(req) for req in requests]
        if observe(old, requests) != expected or observe(new, requests) != expected:
            raise ValueError("native hash views differ from independent byte oracle")
        # Change only the native writer's order. Re-observe; the namespace helper
        # is unchanged. Keep both native source trees inaccessible to the projection.
        mutant = work / "source-mutant"
        run("git", "clone", "--quiet", "--local", "--no-checkout", original, mutant)
        run("git", "checkout", "--quiet", "--detach", PIN, cwd=mutant)
        src = mutant / "crates/marmish-stream/src/copy_protocol.rs"
        lines = src.read_text().splitlines(keepends=True)
        start, = [i for i, line in enumerate(lines) if line.startswith("fn encode_launch(")]
        end = next(i for i in range(start + 1, len(lines)) if lines[i].strip() == "}")
        first, = [i for i in range(start, end) if lines[i].strip().endswith(".extend(v.transfer);")]
        second, = [i for i in range(start, end) if lines[i].strip().endswith(".extend(v.client);")]
        lines[first], lines[second] = lines[second], lines[first]
        src.write_text("".join(lines))
        selection = read(DATA / "selection.json")
        mutant_expansion = expanded(mutant, work / "mutant-expanded.rs")
        selection["sourceSha256"] = sha(mutant_expansion)
        write(work / "mutant-selection.json", selection)
        changed = work / "changed"
        run(cli, "recover", "record-writer", mutant_expansion, work / "mutant-selection.json", changed)
        slots = read(changed / "graph.json")["fields"]
        view["fields"][:-1] = [{"field": slot["field"], "encoding": slot["encoding"]} for slot in slots]
        original_contract_hash = sha(contract_path)
        write(contract_path, contract)
        pin["sha256"] = sha(contract_path)
        if pin["sha256"] == original_contract_hash:
            raise ValueError("source reordering failed to change the contract")
        write(manifest_path, manifest)
        mutant_expansion.unlink()
        altered_product = sealed_build(work / "mutant-build", cli, manifest_path, original, framework, mutant)
        changed_original = consumer(mutant / "crates/marmish-stream", work / "mutant-original", mutant / "Cargo.lock", oracle.CALLER)
        changed_candidate = consumer(altered_product, work / "mutant-candidate", altered_product / "Cargo.lock", oracle.CALLER)
        altered_expected = [dict(row, digest=oracle.expected(req, swapped=True)["digest"]) for req, row in zip(requests, expected)]
        if observe(changed_original, requests) != altered_expected or observe(changed_candidate, requests) != altered_expected:
            raise ValueError("changed writer diverges from independent byte oracle")
        different = sum(a != b for a, b in zip(expected, altered_expected))
        if not different:
            raise ValueError("mutation failed to affect native results")
        write(args.receipt, {"reference": PIN, "framework": FRAMEWORK, "sourceSha256": sha(original / "crates/marmish-stream/src/copy_protocol.rs"),
            "layoutSha256": sha(observed / "graph.json"), "observationSha256": sha(observed / "observation.json"),
            "contractSha256": original_contract_hash, "mutatedContractSha256": sha(contract_path),
            "mutatedCases": len(requests), "changedResults": different, "packageLockSha256": sha(generated / "package.lock"),
            "recoveredFieldSlots": len(layout["fields"]), "nativeCases": len(requests),
            "platform": run("rustc", "+1.97.1", "-vV").split("host: ")[1].splitlines()[0],
            "originalAndCompanionReadDeniedDuringBuild": True})


if __name__ == "__main__":
    main()
