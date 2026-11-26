"""Recover the pinned auxiliary declaration and compose a graph-only successor."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2] / "oddlang/portable"
BASE = ROOT / "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"
WORKSPACE = ROOT / "workspace-v7-offline-cli.json"
AUXILIARY = ROOT / "native-cli-terminal-v1"
NATIVE = ROOT / "native-offline-cli-package.json"
SOURCE = "crates/marmish-native/src/rsync_cli.rs"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
SOURCE_SHA = "4c40daed0c9cc0d11e198509729774d63bec240cebc8b6d9f0228ee911be2ad4"


def sha(content):
    return hashlib.sha256(content).hexdigest()


def pin(path):
    return {"path": path.relative_to(ROOT).as_posix(), "sha256": sha(path.read_bytes())}


def encode(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()


def write(path, data, check):
    if check:
        if path.read_bytes() != data:
            raise ValueError(f"composition differs at {path}")
    else:
        path.write_bytes(data)


def compose(compiler, original, check=True):
    content = subprocess.check_output(["git", "-C", original, "show", f"{ORIGINAL}:{SOURCE}"])
    if sha(content) != SOURCE_SHA:
        raise ValueError("auxiliary source pin differs")
    with tempfile.TemporaryDirectory(prefix="offline-cli-recovery-") as temp:
        source = Path(temp) / "parser.rs"
        graph = Path(temp) / "parser.json"
        source.write_bytes(content)
        subprocess.run([compiler, "recover", "cli-surface", source, AUXILIARY / "selection.json", graph], check=True)
        auxiliary = json.loads(graph.read_bytes())
    if auxiliary["version"] or auxiliary.get("external") or auxiliary["root"].get("subcommands"):
        raise ValueError("unexpected auxiliary parser profile")
    grammar = json.loads((ROOT / "native-cli-v1/graph.json").read_bytes())
    grammar["terminal_subparsers"] = {"rsh": auxiliary["root"]}
    grammar["gaps"].append("Direct rsh help and usage project the auxiliary declaration. Prefix scanning, separator policy and the successful auxiliary parse remain uncovered.")
    output = AUXILIARY / "graph.json"
    write(output, encode(grammar), check)
    workspace = json.loads(BASE.read_bytes())
    native = workspace["members"]["marmish-native"]
    package_pin = native["manifest"]
    package_path = ROOT / package_pin["path"]
    if pin(package_path) != package_pin:
        raise ValueError("base native package changed")
    package = json.loads(package_path.read_bytes())
    for component in package["components"]:
        if component["kind"] == "cli-surface":
            component["graph"] = pin(output)
            base_overlay = ROOT / component["overlay"]["path"]
            if pin(base_overlay) != component["overlay"]:
                raise ValueError("base CLI overlay changed")
            overlay = json.loads(base_overlay.read_bytes())
            operation = overlay["operations"].pop("rsh")
            if operation in overlay.get("handlers", {}):
                raise ValueError("auxiliary parser cannot replace a bound operation")
            qualified_overlay = AUXILIARY / base_overlay.name
            write(qualified_overlay, encode(overlay), check)
            component["overlay"] = pin(qualified_overlay)
    write(NATIVE, encode(package), check)
    native["manifest"] = pin(NATIVE)
    write(WORKSPACE, encode(workspace), check)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("compiler", type=lambda value: Path(value).resolve())
    cli.add_argument("original", type=Path)
    cli.add_argument("--write", action="store_true")
    args = cli.parse_args()
    compose(args.compiler, args.original, check=not args.write)
