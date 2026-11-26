"""Collect public API evidence from the pinned engine library and one source mutation."""

import argparse
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
HELPER = HERE.parent / "gap-range-record" / "verify.py"
REFERENCE = runpy.run_path(str(HELPER))
MODEL = REFERENCE["MODEL"]
PIN = REFERENCE["PIN"]
sha = REFERENCE["sha"]


def emit(original, target, output):
    REFERENCE["clean"](original, PIN)
    if target.is_relative_to(original) or output.is_relative_to(original):
        raise ValueError("use a target and evidence directory outside the original checkout")
    profile_env = {"RUSTC_BOOTSTRAP": "1", "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_INCREMENTAL": "0"}
    env = {**REFERENCE["ENV"], **profile_env}
    version = subprocess.check_output(["rustdoc", "+1.97.1", "-vV"], env=env, text=True).splitlines()
    host, = [line.removeprefix("host: ") for line in version if line.startswith("host: ")]
    output.mkdir(parents=True, exist_ok=False)
    args = ["cargo", "+1.97.1", "rustdoc", "--locked", "--offline", "--lib", "-p", "marmish-engine",
            "--target", host, "--target-dir", str(target), "--", "-Z", "unstable-options", "--output-format", "json", "--document-private-items"]
    rows = {}
    with tempfile.TemporaryDirectory(prefix="odd-range-doc-original-") as work:
        mutant = REFERENCE["mutate_original"](original, Path(work).resolve())
        for label, tree in (("baseline", original), ("mutation", mutant)):
            source = tree / MODEL
            before = sha(source)
            subprocess.run(args, cwd=tree, env=env, check=True)
            result = target / host / "doc" / "marmish_engine.json"
            document = json.loads(result.read_bytes())
            if document["format_version"] != 57 or not document["includes_private"]:
                raise ValueError("unexpected rustdoc evidence profile")
            root_item = document["index"][str(document["root"])]
            if root_item["name"] != "marmish_engine" or sha(source) != before:
                raise ValueError("unexpected engine documentation inputs")
            dest = output / (label + ".json")
            shutil.copyfile(result, dest)
            rows[label] = {"sourceSha256": before, "rustdocSha256": sha(dest), "rustdocFile": dest.name}
        status = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=mutant)
        if status != b" M crates/marmish-engine/src/model.rs\n":
            raise ValueError("unexpected changes to the mutant checkout")
    REFERENCE["clean"](original, PIN)
    if rows["baseline"]["rustdocSha256"] == rows["mutation"]["rustdocSha256"]:
        raise ValueError("rustdoc output did not distinguish the two builds")
    evidence = {"format": "marmish-linked-range-rustdoc-v1", "original": PIN, "host": host,
                "source": MODEL.as_posix(), "cargoLockSha256": sha(original / "Cargo.lock"),
                "crateRootSha256": sha((original / MODEL).with_name("lib.rs")),
                "rustdoc": version, "runnerSha256": sha(Path(__file__)), "helperSha256": sha(HELPER),
                "cargoArgs": ["<target-dir>" if value == str(target) else value for value in args],
                "profileEnvironment": profile_env, "inputs": rows}
    (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "target", "output"):
        parser.add_argument(name, type=lambda s: Path(s).resolve())
    args = parser.parse_args()
    emit(args.original, args.target, args.output)
