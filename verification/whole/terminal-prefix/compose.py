"""Replay the original prefix policy and compose the latest seven-member workspace."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BASE = "workspace-v7-converged-diagnostics-cli"
OUTPUT = "workspace-v7-terminal-prefix"
DOMAIN = PORTABLE / "native-cli-prefix-v1"
PACKAGE = PORTABLE / "native-terminal-prefix-package.json"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = "crates/marmish-native/src/rsync_cli.rs"
SOURCE_SHA = "4c40daed0c9cc0d11e198509729774d63bec240cebc8b6d9f0228ee911be2ad4"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(record):
    return (json.dumps(record, indent=2, ensure_ascii=False) + "\n").encode()


def load(path):
    return json.loads(path.read_bytes())


def pin(path, contents):
    return {"path": path.relative_to(PORTABLE).as_posix(), "sha256": sha(contents)}


def write(path, contents, check):
    if check:
        if path.read_bytes() != contents:
            raise ValueError(f"changed sealed composition file: {path}")
    else:
        path.write_bytes(contents)


def compose(compiler, original, check):
    with tempfile.TemporaryDirectory(prefix="terminal-prefix-recovery-") as temp:
        source = Path(temp) / "oracle.rs"
        policy = Path(temp) / "policy.json"
        data = subprocess.check_output(["git", "-C", original, "show", f"{ORIGINAL}:{SOURCE}"])
        if sha(data) != SOURCE_SHA:
            raise ValueError("prefix recovery source changed")
        source.write_bytes(data)
        subprocess.run([compiler, "recover", "cli-terminal-prefix", source, DOMAIN / "selection.json", policy], check=True)
        recovered = load(policy)
    workspace = load(PORTABLE / f"{BASE}.json")
    native = workspace["members"]["marmish-native"]
    package_path = PORTABLE / native["manifest"]["path"]
    if pin(package_path, package_path.read_bytes()) != native["manifest"]:
        raise ValueError("parent native package pin changed")
    package = load(package_path)
    commands = [component for component in package["components"] if component["kind"] == "cli-surface"]
    first, second = commands
    if first["graph"] != second["graph"]:
        raise ValueError("parent CLI grammar differs by role")
    graph_path = PORTABLE / first["graph"]["path"]
    if pin(graph_path, graph_path.read_bytes()) != first["graph"]:
        raise ValueError("parent CLI graph pin changed")
    graph = load(graph_path)
    graph["terminal_prefixes"] = {"rsh": recovered}
    graph["gaps"] = [entry for entry in graph["gaps"]
                     if not entry.startswith("Direct rsh help and usage")]
    graph["gaps"].append("Leading root option policy applies to auxiliary terminal responses. Malformed prefixes, separators and successful auxiliary invocation are outside this projection.")
    graph_path = DOMAIN / "graph.json"
    new = encode(graph)
    write(graph_path, new, check)
    for component in commands:
        component["graph"] = pin(graph_path, new)
    manifest = encode(package)
    write(PACKAGE, manifest, check)
    native["manifest"] = pin(PACKAGE, manifest)
    write(PORTABLE / f"{OUTPUT}.json", encode(workspace), check)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=lambda p: Path(p).resolve())
    parser.add_argument("original", type=Path)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    compose(args.compiler, args.original, check=not args.write)
