#!/usr/bin/env python3
"""Replay the finite Rust error API against pinned original and sealed generation."""
import argparse
import difflib
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile

ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang" / "portable"
ERR_PATH = "crates/marmish-native/src/error.rs"


def run(*argv, cwd=None, env=None):
    return subprocess.run(argv, cwd=cwd, env=env, check=True, stdout=subprocess.PIPE).stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def pin(base, path):
    return {"path": str(path.relative_to(base)), "sha256": sha(path.read_bytes())}


def compose(portable, output, graph, overlay):
    native = json.loads((portable / "native-configured-lifecycle-package.json").read_bytes())
    native["components"].append({
        "kind": "finite-error-domain", "module": "error",
        "graph": pin(portable, graph), "overlay": pin(portable, overlay),
        "visibility": "public", "reexports": ["Error", "Result"]
    })
    native_path = portable / "native-error-package.json"
    write_json(native_path, native)
    workspace = json.loads((portable / "workspace-v6.json").read_bytes())
    lock = (portable / workspace["cargoLock"]["path"]).read_text()
    before, native_entry = lock.split('name = "marmish-native"\n', 1)
    section, remainder = native_entry.split("\n[[package]]", 1)
    if section.count(' "libc",\n') != 1 or '"marmish-stream"' in section:
        raise ValueError("workspace v6 dependency set changed")
    section = section.replace(' "libc",\n', ' "libc",\n "marmish-stream",\n')
    lock_path = portable / "native-error-v1/Cargo.lock"
    lock_path.write_text(before + 'name = "marmish-native"\n' + section + "\n[[package]]" + remainder)
    workspace["cargoLock"] = pin(portable, lock_path)
    workspace["members"]["marmish-native"] = {
        "manifest": pin(portable, native_path), "dependencies": ["marmish-stream"],
    }
    write_json(output, workspace)


def native_probe(workspace, target, output):
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    cargo = run("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "marmish-native",
                "--lib", "--message-format=json", cwd=workspace, env=env)
    libs = {}
    for line in cargo.splitlines():
        message = json.loads(line)
        if message["reason"] == "compiler-artifact":
            name = message["target"]["name"]
            if name in ("marmish_native", "marmish_stream"):
                for path in message["filenames"]:
                    if path.endswith(".rlib"):
                        if name in libs:
                            raise AssertionError(f"duplicate Cargo artifact {name}")
                        libs[name] = Path(path)
    assert sorted(libs) == ["marmish_native", "marmish_stream"], libs
    run("rustc", "+1.97.1", "--edition=2024", HERE / "probe.rs",
        "-L", f"dependency={libs['marmish_stream'].parent}",
        "--extern", f"marmish_stream={libs['marmish_stream']}",
        "--extern", f"marmish_native={libs['marmish_native']}", "-o", output)
    return run(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--original-target", type=Path, required=True)
    parser.add_argument("--generated-target", type=Path, required=True)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    args.oddlang = args.oddlang.resolve()
    args.original = args.original.resolve()
    baseline_graph = PORTABLE / "native-error-v1"
    generated_spec = PORTABLE / "workspace-v6-native-error.json"
    assert run("git", "-C", args.original, "rev-parse", "HEAD").decode().strip() == ORIGINAL
    assert not run("git", "-C", args.original, "status", "--porcelain", "--untracked-files=no")
    pinned_source = run("git", "-C", args.original, "show", f"{ORIGINAL}:{ERR_PATH}")
    assert (args.original / ERR_PATH).read_bytes() == pinned_source
    if pinned_source.count(b'Quota => "capacity"') != 1:
        raise AssertionError("controlled mutation must affect exactly one category")
    mutated_source = pinned_source.replace(b'Quota => "capacity"', b'Quota => "transfer_limit"')
    args.output.mkdir(parents=True)
    receipt = {
        "format": "marmish-native-error-evidence-v1",
        "originalRevision": ORIGINAL,
        "frameworkRevision": args.framework_revision,
        "rustc": run("rustc", "+1.97.1", "-vV").decode().splitlines(),
        "callerSha256": sha((HERE / "probe.rs").read_bytes()),
        "scope": "finite error API and display/source/From behavior; default debug build",
        "variants": 10,
        "typedCategories": {"copy": 14, "wire": 10},
        "profiles": {},
    }
    with tempfile.TemporaryDirectory(prefix="mne-") as temp:
        scratch = Path(temp)
        baseline = scratch / "baseline"
        run(args.oddlang, "recover", "finite-error-domain", args.original / ERR_PATH,
            HERE / "selection.json", baseline)
        mutant_original = scratch / "original-mutant"
        run("git", "clone", "--shared", "--no-checkout", str(args.original), str(mutant_original))
        run("git", "-C", mutant_original, "checkout", "--detach", ORIGINAL)
        (mutant_original / ERR_PATH).write_bytes(mutated_source)
        git_env = os.environ.copy()
        for role in ("AUTHOR", "COMMITTER"):
            git_env[f"GIT_{role}_NAME"] = "Qualification Fixture"
            git_env[f"GIT_{role}_EMAIL"] = "fixture@example.invalid"
            git_env[f"GIT_{role}_DATE"] = "2026-11-26T12:00:00Z"
        run("git", "-c", "commit.gpgSign=false", "-C", mutant_original, "commit",
            "--only", "-m", "Change the copy quota category", "--", ERR_PATH, env=git_env)
        assert run("git", "-C", mutant_original, "diff", "--name-only", ORIGINAL).decode().splitlines() == [ERR_PATH]
        selection = json.loads((HERE / "selection.json").read_bytes())
        selection["sourceSha256"] = sha(mutated_source)
        mutant_selection = scratch / "mutant-selection.json"
        write_json(mutant_selection, selection)
        mutant = scratch / "mutant"
        run(args.oddlang, "recover", "finite-error-domain", mutant_original / ERR_PATH,
            mutant_selection, mutant)
        for name in ("graph.json", "rust.json", "observation.json"):
            assert (baseline / name).read_bytes() == (baseline_graph / name).read_bytes()
        assert (baseline / "rust.json").read_bytes() == (mutant / "rust.json").read_bytes()
        before = json.loads((baseline / "graph.json").read_bytes())
        after = json.loads((mutant / "graph.json").read_bytes())
        changed = [(table, member) for table, entries in before["categories"].items()
                   for member, text in entries.items() if after["categories"][table][member] != text]
        assert len(changed) == 1
        table, member = changed[0]
        assert before["categories"][table][member] == "capacity"
        assert after["categories"][table][member] == "transfer_limit"
        before["categories"][table][member] = "transfer_limit"
        assert before == after
        for mode, recovered, original in (("baseline", baseline, args.original),
                                          ("mutant", mutant, mutant_original)):
            portable = scratch / ("portable-" + mode)
            shutil.copytree(PORTABLE, portable)
            native_error = portable / "native-error-v1"
            graph, overlay = native_error / "graph.json", native_error / "rust.json"
            shutil.copyfile(recovered / "graph.json", graph)
            local_spec = portable / generated_spec.name
            compose(portable, local_spec, graph, overlay)
            if mode == "baseline":
                for relative in (generated_spec.name, "native-error-package.json",
                                 "native-error-v1/Cargo.lock"):
                    assert (portable / relative).read_bytes() == (PORTABLE / relative).read_bytes()
            generated = scratch / ("generated-" + mode)
            run(args.oddlang, "build", "native-workspace", "rust", local_spec, generated)
            upstream = native_probe(original, args.original_target.resolve(), scratch / "original-probe")
            candidate = native_probe(generated, args.generated_target.resolve(), scratch / "generated-probe")
            if upstream != candidate:
                raise AssertionError("".join(difflib.unified_diff(
                    upstream.decode().splitlines(True), candidate.decode().splitlines(True))))
            expected = "capacity" if mode == "baseline" else "transfer_limit"
            assert len(upstream.splitlines()) == 32
            assert any(line.startswith(f'Copy|Quota|"copy: {expected}"'.encode())
                       for line in upstream.splitlines())
            (args.output / (mode + ".txt")).write_bytes(upstream)
            receipt["profiles"][mode] = {
                "sourceRevision": run("git", "-C", original, "rev-parse", "HEAD").decode().strip(),
                "sourceSha256": sha((original / ERR_PATH).read_bytes()),
                "graphSha256": sha(graph.read_bytes()),
                "overlaySha256": sha(overlay.read_bytes()),
                "workspaceSpecSha256": sha(local_spec.read_bytes()),
                "workspaceLockSha256": sha((generated / "workspace.lock").read_bytes()),
                "originalAndGeneratedOutputSha256": sha(upstream),
                "matchedRows": 32,
                "quotaDisplay": expected,
            }
        assert (args.output / "baseline.txt").read_bytes() != (args.output / "mutant.txt").read_bytes()
    assert not run("git", "-C", args.original, "status", "--porcelain", "--untracked-files=no")
    write_json(args.output / "receipt.json", receipt)


if __name__ == "__main__":
    main()
