"""Recover the policy from the pinned native source and regenerate the selected workspace."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parent
REVISION = "af54339d250290490d62d4475a77539b7e739071"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def write(path, document):
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def reference(path):
    return {"path": path.relative_to(PORTABLE).as_posix(), "sha256": digest(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    compiler = args.compiler.resolve()
    original = args.original.resolve()
    destination = args.destination.resolve()
    if destination.exists():
        parser.error("choose a new workspace destination")
    revision = subprocess.check_output(["git", "-C", original, "rev-parse", "HEAD"], text=True).strip()
    if revision != REVISION:
        parser.error(f"expected Marmish revision {REVISION}")
    engine = original / "crates/marmish-engine/src/policy.rs"
    if digest(engine) != "bfaa35f2aa5ff29e48098f942c011a21ec66449c7c641d163b9cee0e6ff2fa2d":
        parser.error("policy engine differs from the reviewed decision binding")
    recovered = HERE / "schema.json"
    with tempfile.TemporaryDirectory() as scratch:
        extracted = Path(scratch) / "schema.json"
        subprocess.run([compiler, "recover", "restriction-lists",
                        original / "crates/marmish-native/src/command_config.rs",
                        HERE / "selection.json", extracted], check=True)
        shutil.copyfile(extracted, recovered)
    foreground = read(PORTABLE / "bounded-owner-prepare-v1/foreground.json")
    foreground["room_prepare"]["restriction_lists"] = {
        "schema": read(recovered), "decision": read(HERE / "decision.json")
    }
    graph = HERE / "foreground.json"
    write(graph, foreground)
    package = read(PORTABLE / "marmish-native-retained-poll-package.json")
    owner, = (item for item in package["components"] if item.get("module") == "foreground")
    owner["graph"] = reference(graph)
    manifest = PORTABLE / "marmish-native-command-policy-package.json"
    write(manifest, package)
    workspace = read(PORTABLE / "workspace-retained-poll.json")
    workspace["members"]["marmish-native"]["manifest"] = reference(manifest)
    release = PORTABLE / "workspace-command-policy.json"
    write(release, workspace)
    subprocess.run([compiler, "build", "native-workspace", "rust", release, destination], check=True)
    shutil.copyfile(destination / "workspace.lock", PORTABLE / "workspace-command-policy/workspace.lock")


if __name__ == "__main__":
    main()
