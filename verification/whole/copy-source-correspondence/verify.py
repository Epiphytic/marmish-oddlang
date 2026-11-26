"""Recover the released copy field correspondence and qualify a native source mutation."""
import argparse
import json
import os
from pathlib import Path
import platform
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import PIN, ROOT, consumer, observe, read, run, sha, write
from verify import linux_project, staged

CALLER = HERE.parent / "copy-field-relations/caller.rs"
PORTABLE = ROOT / "oddlang/portable"
RELEASE = PORTABLE / "stream-copy-source-correspondence-package.json"
DATA = PORTABLE / "copy-source-correspondence-v1"
RELATION = "urn:marmish:copy:launch:matches-manifest"
METHOD = "crate::copy_protocol::Launch::matches_manifest"
SOURCE = Path("crates/marmish-stream/src/copy_protocol.rs")
BINDING = PORTABLE / "copy-field-relations-v1/rust.json"
RELEASED = PORTABLE / "copy-field-relations-v1/graph.json"
CONTRACT = runpy.run_path(str(CALLER.parent / "verify.py"))


def checked(*command, cwd=None, env=None):
    return subprocess.run(command, cwd=cwd, env=env, check=True)


def recover_paths(directory):
    directory.mkdir(parents=True, exist_ok=True)
    return (directory / "evidence.json", directory / "selection.json", directory / "graph.json")


def recover(framework, cli, checkout, target, output):
    evidence, selection, graph = recover_paths(output)
    source = checkout / SOURCE
    with evidence.open("w") as sink:
        subprocess.run(["bash", framework / "verification/rust-recovery/run.sh",
                        checkout / "crates/marmish-stream/Cargo.toml", "marmish-stream",
                        "lib", "marmish_stream", target, "default"],
                       env=dict(os.environ, ODD_RECOVERY_FIELD_CONJUNCTION=METHOD), check=True, stdout=sink)
    write(selection, dict(format="oddlang-field-conjunction-selection-v1", relation=RELATION,
                          sourceSha256=sha(source), evidenceSha256=sha(evidence)))
    checked(cli, "recover", "field-conjunction", checkout / "crates/marmish-stream", source, evidence, selection,
            DATA / "template.json", BINDING, graph)
    return graph


def sealed_project(directory, cli, manifest, dest, original, mutant):
    if platform.system() == "Linux":
        return linux_project(cli, manifest, dest, original, CALLER, extra_sources=(mutant,))
    assert platform.system() == "Darwin", "source-isolated generation needs Linux or macOS"
    profile = "(version 1)(allow default)(deny network*)" + "".join(
        "(deny file-read* (subpath " + json.dumps(str(path)) + "))" for path in (ROOT, original, mutant))
    sandbox = ["/usr/bin/sandbox-exec", "-p", profile]
    for path in (original / SOURCE, mutant / SOURCE, CALLER, DATA / "evidence.json"):
        control = subprocess.run([*sandbox, "/bin/cat", path], capture_output=True)
        assert control.returncode and b"Operation not permitted" in control.stderr
    assert not list(manifest.parent.rglob("*.rs"))
    assert not list(manifest.parent.rglob("evidence.json"))
    checked(*sandbox, cli, "build", "native-package", "rust", manifest, dest, cwd=directory)
    checked(*sandbox, "cargo", "+1.97.1", "build", "--locked", "--offline", "--release", cwd=dest)
    return True


def archive_at(checkout, revision, destination):
    destination.mkdir()
    archive = subprocess.check_output(["git", "archive", revision], cwd=checkout)
    subprocess.run(["tar", "-xf", "-", "-C", destination], input=archive, check=True)


def mutation(original, mutant):
    archive_at(original, PIN, mutant)
    source = mutant / SOURCE
    text = source.read_text()
    old = "&& self.transfer == manifest.transfer"
    assert text.count(old) == 1
    source.write_text(text.replace(old, "&& self.transfer == manifest.server"))


def oracle_rows():
    all_requests, expected = CONTRACT["corpus"]()
    original_expected = [item for req, item in zip(all_requests, expected, strict=True)
                         if req["operation"] == "match"]
    requests = [req for req in all_requests if req["operation"] == "match"]
    base = CONTRACT["manifest"]["corpus"]()[0][0]
    assert base["server"] != base["transfer"]
    # Cases where the two native revisions disagree in both directions.
    paired = dict(base, server=base["transfer"], transfer=base["server"])
    launch = {key: paired[key] for key in CONTRACT["KEYS"]} | dict(transfer=paired["server"], mode="create")
    request = dict(operation="match", launch=CONTRACT["launch_wire"](launch),
                   manifest=list(CONTRACT["manifest"]["encode"](paired)))
    mutated_expected = [False] * len(requests)
    return requests + [request], original_expected + [False], mutated_expected + [True]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for label in ("original", "framework", "receipt"):
        parser.add_argument(label, type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    framework_pin = read(HERE / "reference-aarch64-apple-darwin.json")["framework"]
    assert run("git", "rev-parse", "HEAD", cwd=framework) == framework_pin
    assert run("git", "rev-parse", "HEAD", cwd=original) == PIN
    assert not run("git", "status", "--porcelain", cwd=original)
    rust = run("rustc", "+1.97.1", "-vV")
    target = next(line.split()[1] for line in rust.splitlines() if line.startswith("host:"))
    with tempfile.TemporaryDirectory(prefix="copy-source-") as directory:
        work = Path(directory).resolve()
        sealed_framework = work / "framework"
        archive_at(framework, framework_pin, sealed_framework)
        tools = work / "tools"
        checked("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
                "--target-dir", tools, cwd=sealed_framework)
        cli = tools / "debug/oddlang"
        mutant = work / "mutant-source"
        mutation(original, mutant)
        base_graph = recover(sealed_framework, cli, original, target, work / "original-recovery")
        mutated_graph = recover(sealed_framework, cli, mutant, target, work / "mutant-recovery")
        expected_relation = read(RELEASED)
        for relation in expected_relation["relations"].values():
            relation.setdefault("failure", None)
        for signature in expected_relation["calls"].values():
            signature.setdefault("output", None)
        assert read(base_graph) == expected_relation
        for filename in ("graph.json", "evidence.json", "selection.json"):
            assert (base_graph.parent / filename).read_bytes() == (DATA / filename).read_bytes(), filename
        rows = expected_relation["relations"][RELATION]["steps"]
        assert rows[1]["right"]["field"].endswith(":transfer")
        rows[1]["right"]["field"] = rows[1]["right"]["field"].removesuffix(":transfer") + ":server"
        assert read(mutated_graph) == expected_relation
        package = staged(work, RELEASE)
        native = work / "base-package"
        denied = sealed_project(work, cli, package, native, original, mutant)
        package_manifest = read(package)
        pin = package_manifest["components"][-1]["graph"]
        (package.parent / pin["path"]).write_bytes(mutated_graph.read_bytes())
        pin["sha256"] = sha(mutated_graph)
        write(package, package_manifest)
        altered = work / "altered-package"
        assert sealed_project(work, cli, package, altered, original, mutant)
        requests, normal, changed = oracle_rows()
        for label, root, lock, expect in (
            ("reference", original / SOURCE.parents[1], original / "Cargo.lock", normal),
            ("recovered", native, native / "Cargo.lock", normal),
            ("mutant-reference", mutant / SOURCE.parents[1], mutant / "Cargo.lock", changed),
            ("mutant-recovered", altered, altered / "Cargo.lock", changed),
        ):
            executable = consumer(root, work / label, lock, CALLER)
            assert observe(executable, requests) == expect, label
        original_package = native / "package.lock"
        mutant_package = altered / "package.lock"
        assert original_package.read_bytes() == (DATA / "package.lock").read_bytes()
        original_component = read(original_package)["components"][-1]
        mutant_component = read(mutant_package)["components"][-1]
        assert original_component["kind"] == mutant_component["kind"] == "field-relation"
        assert original_component["sourceSha256"] != mutant_component["sourceSha256"]
        write(args.receipt, dict(original=PIN, framework=framework_pin, rust=rust,
              comparisons=len(rows), casesPerImplementation=len(requests),
              changedCases=sum(x != y for x, y in zip(normal, changed, strict=True)),
              readDenied=denied, callerSha256=sha(CALLER), originalGraphSha256=sha(base_graph), mutatedGraphSha256=sha(mutated_graph),
              originalEvidenceSha256=sha(base_graph.parent / "evidence.json"),
              mutatedEvidenceSha256=sha(mutated_graph.parent / "evidence.json"),
              originalGeneratedRelationSha256=original_component["sourceSha256"],
              mutantGeneratedRelationSha256=mutant_component["sourceSha256"],
              originalPackageLockSha256=sha(original_package), mutantPackageLockSha256=sha(mutant_package)))
        print("Qualified pinned native copy field correspondence and native source mutation.")


if __name__ == "__main__":
    main()
