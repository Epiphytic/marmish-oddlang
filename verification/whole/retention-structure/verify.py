"""Recover original and mutated Rust ranges, lower both, and check native core."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
RUST = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
TEST = Path("crates/marmish-core/tests/contracts.rs")
CONFIG = Path("crates/marmish-core/src/config.rs")
OLD = "retention must be between one second and 365 days"
NEW = "retention must be between one second and seven days"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def call(*args, cwd=None, env=None):
    result = subprocess.run(
        args, cwd=cwd, env=env, text=True, capture_output=True, timeout=600
    )
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def generated(cli, stage, recovered, env):
    manifest = json.loads((PORTABLE / "core-package.json").read_bytes())
    inputs = stage / "inputs"
    inputs.mkdir(parents=True)
    for pin in [
        manifest["cargoLock"],
        *[row[key] for row in manifest["components"] for key in ("graph", "overlay")],
    ]:
        target = inputs / pin["path"]
        target.parent.mkdir(exist_ok=True, parents=True)
        source = PORTABLE / pin["path"]
        assert sha(source) == pin["sha256"]
        shutil.copyfile(source, target)
    component = next(
        row for row in manifest["components"] if row["kind"] == "value-domain"
    )
    shutil.copyfile(recovered / "domain.ttl", inputs / component["graph"]["path"])
    component["graph"]["sha256"] = sha(recovered / "domain.ttl")
    write(inputs / "package.json", manifest)
    out = stage / "generated"
    call(cli, "build", "native-package", "rust", inputs / "package.json", out, env=env)
    return out


def exercise(manifest, work, triple, env):
    target = work / "target"
    result = call(
        "cargo",
        "+1.97.1",
        "test",
        "--release",
        "--offline",
        "--locked",
        "--manifest-path",
        manifest,
        "--target-dir",
        target,
        "--target",
        triple,
        "-p",
        "marmish-core",
        "--test",
        "contracts",
        env=env,
    )
    tests = sorted(re.findall(r"^test (\w+) \.\.\. ok$", result, re.M))
    assert len(tests) == 7, result
    deps = target / triple / "release/deps"
    (library,) = deps.glob("libmarmish_core-*.rlib")
    caller = work / "caller"
    call(
        "rustc",
        "+1.97.1",
        "--edition",
        "2021",
        "--target",
        triple,
        "--extern",
        f"marmish_core={library}",
        "-L",
        f"dependency={deps}",
        "-L",
        f"dependency={target / 'release/deps'}",
        HERE / "caller.rs",
        "-o",
        caller,
        env=env,
    )
    return tests, call(caller, cwd=work, env=env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    cli = args.oddlang.resolve()
    original = args.original.resolve()
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(
            ("RUST", "CARGO_ENCODED_", "CARGO_TARGET_", "CARGO_PROFILE_")
        )
        and k not in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET")
    }
    env["RUSTUP_TOOLCHAIN"] = "1.97.1"
    version = call("rustc", "+1.97.1", "-vV", env=env)
    assert f"commit-hash: {RUST}\n" in version
    triple = next(
        line.split()[1] for line in version.splitlines() if line.startswith("host: ")
    )
    assert call("git", "rev-parse", "HEAD", cwd=original).strip() == ORIGINAL
    assert not call("git", "status", "--porcelain", cwd=original)
    unchanged = (original / TEST).read_bytes()
    args.output.mkdir(parents=True, exist_ok=False)
    out = args.output.resolve()
    with tempfile.TemporaryDirectory(prefix="retention-structure-") as temp:
        stage = Path(temp).resolve()
        mutant = stage / "mutant"
        call("git", "clone", "--shared", "--no-checkout", original, mutant)
        call("git", "checkout", "--detach", ORIGINAL, cwd=mutant)
        config = mutant / CONFIG
        text = config.read_text()
        assert text.count("365 * 86_400") == 1 and text.count(OLD) == 1
        config.write_text(text.replace("365 * 86_400", "7 * 86_400").replace(OLD, NEW))
        call(
            "git",
            "-c",
            "user.name=Structural Control",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-am",
            "Change retained interval",
            cwd=mutant,
            env={
                **env,
                "GIT_AUTHOR_DATE": "2026-11-26T00:00:00Z",
                "GIT_COMMITTER_DATE": "2026-11-26T00:00:00Z",
            },
        )
        cases = {}
        for label, checkout in (("baseline", original), ("mutated", mutant)):
            case = stage / label
            case.mkdir()
            revision = call("git", "rev-parse", "HEAD", cwd=checkout).strip()
            selection = json.loads((HERE / "selection.json").read_bytes())
            selection["sourceRevision"] = revision
            selection_path = case / "selection.json"
            write(selection_path, selection)
            recovered = out / label
            manifest = checkout / "crates/marmish-core/Cargo.toml"
            call(
                cli,
                "recover",
                "value-domain-structure",
                manifest,
                selection_path,
                recovered,
                env=env,
            )
            graph = (recovered / "domain.ttl").read_bytes()
            if label == "baseline":
                assert graph == (PORTABLE / "core-config-v1/domain.ttl").read_bytes()
            else:
                assert b"c:maximum 604800" in graph and NEW.encode() in graph
            projected = generated(cli, case, recovered, env)
            (projected / "tests").mkdir()
            (projected / "tests/contracts.rs").write_bytes(unchanged)
            assert (checkout / TEST).read_bytes() == unchanged
            test_names, actual = exercise(manifest, case / "original", triple, env)
            repeatedly, candidate = exercise(
                projected / "Cargo.toml", case / "candidate", triple, env
            )
            assert test_names == repeatedly and actual == candidate
            expected = (
                "Some(31536000) => Some(31536000)"
                if label == "baseline"
                else f"Some(31536000) => RetentionOutOfRange: {NEW}"
            )
            assert expected in actual
            (recovered / "caller.txt").write_text(actual)
            cases[label] = {
                "sourceRevision": revision,
                "tests": test_names,
                "graphSha256": sha(recovered / "domain.ttl"),
                "callerOutputSha256": sha(recovered / "caller.txt"),
                "packageLockSha256": sha(projected / "package.lock"),
            }
        assert (
            cases["baseline"]["callerOutputSha256"]
            != cases["mutated"]["callerOutputSha256"]
        )
        write(
            out / "receipt.json",
            {
                "frameworkRevision": args.framework_revision,
                "originalRevision": ORIGINAL,
                "compilerCommit": RUST,
                "target": triple,
                "unchangedTestSha256": sha(original / TEST),
                "callerSha256": sha(HERE / "caller.rs"),
                "cases": cases,
            },
        )
    assert not call("git", "status", "--porcelain", cwd=original)


if __name__ == "__main__":
    main()
