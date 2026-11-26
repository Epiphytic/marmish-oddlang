#!/usr/bin/env python3
"""Compare a structurally recovered report with independently compiled native output."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang" / "portable"
PIN = "af54339d250290490d62d4475a77539b7e739071"
ENGINE_SOURCE = "crates/marmish-engine/src/store.rs"
GRAPH_DIR = "native-build-report-recovered-v1"
PACKAGE = "native-build-report-recovered-package.json"
WORKSPACE = "workspace-v7-recovered-report.json"
ROLES = ("marmish", "marmishd")


def run(*args, cwd=None, env=None):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, cwd=cwd, env=env).stdout


def data(path):
    return json.loads(path.read_bytes())


def put(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pin(base, path):
    return {"path": str(path.relative_to(base)), "sha256": sha(path)}


def compose(portable, recovered):
    original_graph = data(portable / "native-build-report-v1/graph.json")
    graph = data(recovered / "graph.json")
    graph["guards"] = original_graph["guards"]
    path = portable / GRAPH_DIR / "graph.json"
    put(path, graph)
    package = data(portable / "native-build-report-package.json")
    report, = (entry for entry in package["components"] if entry["kind"] == "build-report")
    report["graph"] = pin(portable, path)
    package_path = portable / PACKAGE
    put(package_path, package)
    workspace = data(portable / "workspace-v7.json")
    workspace["members"]["marmish-native"]["manifest"] = pin(portable, package_path)
    workspace_path = portable / WORKSPACE
    put(workspace_path, workspace)
    return workspace_path


def binaries(root, target, destination, original):
    env = {key: value for key, value in os.environ.items()
           if key not in {"MARMISH_BUILD_COMMIT", "MARMISH_BUILD_TARGET", "CARGO_BUILD_TARGET"}}
    if original:
        # Both test revisions represent the same release input, isolating the declaration mutation.
        env["MARMISH_BUILD_COMMIT"] = PIN
    target.mkdir(parents=True, exist_ok=True)
    destination.mkdir()
    with (target / ".release-info-structure.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX)
        artifacts = ROLES + ("marmish-worker",) if original else ROLES
        selected = [arg for role in artifacts for arg in ("--bin", role)]
        run("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "marmish-native",
            *selected, "--target-dir", target, cwd=root, env=env)
        for role in artifacts:
            shutil.copy2(target / "debug" / role, destination / role)
    return destination


def check_types(source, target):
    root = source / "crates/marmish-native/src/lib.rs"
    original = root.read_bytes()
    target.mkdir(parents=True, exist_ok=True)
    with (target / ".release-info-structure.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX)
        try:
            root.write_bytes(original + b"\n" + (HERE / "types.rs").read_bytes())
            env = os.environ.copy()
            env["MARMISH_BUILD_COMMIT"] = PIN
            env.pop("CARGO_BUILD_TARGET", None)
            run("cargo", "+1.97.1", "check", "--locked", "--offline", "-p",
                "marmish-native", "--lib", "--target-dir", target, cwd=source, env=env)
        finally:
            root.write_bytes(original)


def observe(binaries_path, root):
    env = dict(HOME=str(root), TMPDIR=str(root), PATH=os.defpath, LANG="C",
               TARGET="runtime-override", CARGO_PKG_VERSION="runtime-override",
               MARMISH_BUILD_TARGET="runtime-override", MARMISH_BUILD_COMMIT="runtime-override")
    outputs = {}
    for role in ROLES:
        call = subprocess.run([binaries_path / role, "release-info"], cwd=root,
                              env=env, capture_output=True, timeout=25)
        assert call.returncode == 0 and not call.stderr and call.stdout.endswith(b"\n")
        outputs[role] = call.stdout.decode()
    assert not list(root.iterdir()), "release report created files"
    return outputs


def ingest(oddlang, source, output, selection_path):
    config = data(HERE / "selection.json")
    config["sources"] = [pin(source, source / entry["path"]) for entry in config["sources"]]
    put(selection_path, config)
    run(oddlang, "recover", "build-report", source, selection_path, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--original-target", required=True, type=Path)
    parser.add_argument("--generated-target", required=True, type=Path)
    args = parser.parse_args()
    assert args.original_target.resolve() != args.generated_target.resolve()
    args.output.mkdir()
    source = args.original.resolve()
    assert run("git", "-C", source, "rev-parse", "HEAD").decode().strip() == PIN
    assert not run("git", "-C", source, "status", "--porcelain", "--untracked-files=no")
    receipt = {
        "format": "marmish-release-info-structure-v1",
        "frameworkRevision": args.framework_revision,
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "representedRevision": PIN,
        "nativeTypeProbeSha256": sha(HERE / "types.rs"),
        "lanes": {},
    }
    host, = (line.removeprefix("host: ") for line in receipt["rustc"] if line.startswith("host: "))
    with tempfile.TemporaryDirectory(prefix="rri-", dir="/tmp") as temp:
        root = Path(temp)
        mutant = root / "mutant"
        run("git", "clone", "--shared", "--no-checkout", source, mutant)
        run("git", "-C", mutant, "checkout", "--detach", PIN)
        engine = mutant / ENGINE_SOURCE
        old = b"pub(crate) const ENGINE_FORMAT: i64 = 1;"
        assert engine.read_bytes().count(old) == 1
        engine.write_bytes(engine.read_bytes().replace(old, b"pub(crate) const ENGINE_FORMAT: i64 = 4294967305;"))
        env = os.environ.copy()
        for role in ("AUTHOR", "COMMITTER"):
            env[f"GIT_{role}_NAME"] = "Qualification Fixture"
            env[f"GIT_{role}_EMAIL"] = "fixture@example.invalid"
            env[f"GIT_{role}_DATE"] = "2026-11-26T12:00:00Z"
        run("git", "-C", mutant, "-c", "commit.gpgSign=false", "commit", "--only", "-m",
            "Change the reported engine format", "--", ENGINE_SOURCE, env=env)
        assert run("git", "-C", mutant, "diff", "--name-only", PIN).decode().splitlines() == [ENGINE_SOURCE]
        observations = {}
        for name, original in (("baseline", source), ("mutant", mutant)):
            extracted = root / ("extracted-" + name)
            selection = root / (name + "-selection.json")
            ingest(args.oddlang.resolve(), original, extracted, selection)
            evidence = data(extracted / "observation.json")
            portable = root / ("portable-" + name)
            shutil.copytree(PORTABLE, portable)
            workspace = compose(portable, extracted)
            if name == "baseline":
                assert selection.read_bytes() == (HERE / "selection.json").read_bytes()
                for path in (WORKSPACE, PACKAGE, GRAPH_DIR + "/graph.json"):
                    assert (portable / path).read_bytes() == (PORTABLE / path).read_bytes()
            generated = root / ("workspace-" + name)
            run(args.oddlang.resolve(), "build", "native-workspace", "rust", workspace, generated)
            left = binaries(original, args.original_target.resolve() / name, root / ("original-bins-" + name), True)
            right = binaries(generated, args.generated_target.resolve() / name, root / ("generated-bins-" + name), False)
            invocation_dir = root / ("invocation-" + name)
            invocation_dir.mkdir()
            expected = observe(left, invocation_dir)
            actual = observe(right, invocation_dir)
            assert expected == actual and expected["marmish"] == expected["marmishd"], (name, expected, actual)
            record = json.loads(expected["marmish"])
            assert list(record) == ["schema", "version", "target", "commit", "formats"]
            version = tomllib.loads((original / "crates/marmish-native/Cargo.toml").read_text())["package"]["version"]
            assert record["target"] == host and record["version"] == version
            assert record["schema"] == evidence["values"][0] == 1
            assert record["commit"] == PIN
            assert record["formats"] == evidence["values"][-1], (name, record, evidence["values"])
            output = args.output / (name + ".json")
            put(output, expected)
            observations[name] = (data(extracted / "graph.json"), record)
            receipt["lanes"][name] = {
                "sourceRevision": run("git", "-C", original, "rev-parse", "HEAD").decode().strip(),
                "selectionSha256": sha(selection), "sourceObservation": evidence,
                "composedGraphSha256": sha(portable / GRAPH_DIR / "graph.json"),
                "workspaceSpecSha256": sha(workspace),
                "workspaceLockSha256": sha(generated / "workspace.lock"),
                "nativeOutputSha256": sha(output),
            }
        (before, left), (after, right) = (observations["baseline"], observations["mutant"])
        assert left["formats"] == [2, 7, 1, 1, 1] and right["formats"] == [2, 7, 9, 1, 1]
        left["formats"][2] = 9
        assert left == right
        leaf = before["fields"][-1]["source"]["items"][2]["chain"][-1]
        assert leaf["value"] == 1
        leaf["value"] = 4294967305
        assert before == after
        check_types(mutant, args.original_target.resolve() / "mutant")
        assert not run("git", "-C", mutant, "status", "--porcelain", "--untracked-files=no")
    assert not run("git", "-C", source, "status", "--porcelain", "--untracked-files=no")
    put(args.output / "receipt.json", receipt)


if __name__ == "__main__":
    main()
