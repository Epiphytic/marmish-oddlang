"""Compose the read-only catalog and a pinned dev-only upstream test provider."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
FILE = runpy.run_path(str(HERE.parent / "local-file-integrity/verify.py"))
PORTABLE, ORIGINAL, pin = (FILE[name] for name in ("PORTABLE", "PIN", "pin"))
BASE = FILE["MANIFEST"]
MEMBER = FILE["MEMBER"]
SCOPE = "local-file-catalog-v1"
OUTPUT = "workspace-v7-local-file-catalog.json"
PACKAGE = "native-local-file-catalog-package.json"


def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def check_pin(path, descriptor):
    if pin(PORTABLE, path) != descriptor:
        raise ValueError(f"input pin differs: {path.name}")
    return path.read_bytes()


def add_fixture_dependencies(lock, upstream):
    marker = b"[[package]]\n"

    def parts(data):
        first, *rest = data.split(marker)
        packages = [(tomllib.loads(row.decode())["name"], row) for row in rest]
        return first, packages

    header, rows = parts(lock)
    _, original = parts(upstream)
    selected = [(name, data) for name, data in original if name in {"fastrand", "tempfile"}]
    if sorted(name for name, _ in selected) != ["fastrand", "tempfile"]:
        raise ValueError("original test provider inventory changed")
    if any(name in {"fastrand", "tempfile"} for name, _ in rows):
        raise ValueError("test provider unexpectedly reached the predecessor lock")
    changed = []
    for name, data in rows:
        if name == MEMBER:
            label = b' "tokio",\n'
            if data.count(label) != 1:
                raise ValueError("cannot add the fixture-only native edge")
            updated = data.replace(label, b' "tempfile",\n' + label)
            original_entry, updated_entry = [tomllib.loads(blob.decode()) for blob in (data, updated)]
            original_entry["dependencies"].append("tempfile")
            if sorted(original_entry["dependencies"]) != updated_entry["dependencies"]:
                raise ValueError("test dependency changed another native edge")
            data = updated
        changed.append((name, data))
    result = header + b"".join(marker + part for _, part in sorted([*changed, *selected], key=lambda row: row[0]))
    tomllib.loads(result.decode())
    return result


def entries(base):
    data = json.loads((base / BASE).read_bytes())
    native = data["members"][MEMBER]["manifest"]
    package = json.loads(check_pin(base / native["path"], native))
    grammar, = [c for c in package["components"] if c["module"] == "native_manifest"]
    grammar_graph = json.loads(check_pin(base / grammar["graph"]["path"], grammar["graph"]))
    file_effect, = [c for c in package["components"] if c["kind"] == "read-only-file"]
    file_graph = json.loads(check_pin(base / file_effect["graph"]["path"], file_effect["graph"]))
    return data, package, grammar_graph, file_graph


def compose(original, check):
    upstream_lock = subprocess.check_output(["git", "-C", original, "show", f"{ORIGINAL}:Cargo.lock"])
    workspace, native, grammar, files = entries(PORTABLE)
    literal = {row["capture"]: row["text"]
               for line in grammar["rows"] for row in line
               if row["kind"] == "literal" and "capture" in row}
    names = [literal[record["fields"]["name"]["id"]]
             for record in grammar["recordResult"]["entry"]["fields"]["files"]["items"]]
    version, = grammar["textGrammars"]
    spec = {
        "format": "oddlang-local-file-catalog-v1", "id": "urn:marmish:native:read-only-file-catalog:v1",
        "manifestGrammar": grammar["id"], "filePolicy": files["profile"]["id"],
        "fixedCatalog": "manifest", "prefixes": [f"{names[0]}-{family}" for family in version["prefixes"]],
        "candidateNameEncoding": "utf8", "attributeChecks": "fixed-owner-and-mode",
        "suffix": ".manifest", "namingAnchor": names[0], "assetNames": names,
        "maximumCatalogBytes": grammar["maxBytes"], "digest": "sha256",
        "guards": ["unique-name", "bounded-read", "catalog-digest", "parse",
                   "canonical-name", "ordered-assets"],
        "error": {"id": "urn:marmish:native:error:v1:variant:Invalid",
                  "ambiguous": "install_manifest_path", "digest": "install_digest"},
    }

    def emit(relative, bytes_):
        target = PORTABLE / relative
        if check:
            if target.read_bytes() != bytes_:
                raise ValueError(f"changed generated input {relative}")
        else:
            target.parent.mkdir(exist_ok=True)
            target.write_bytes(bytes_)
        return pin(PORTABLE, target)

    graph = emit(f"{SCOPE}/graph.json", encode(spec))
    native["components"].append({
        "kind": "local-file-catalog", "module": "native_manifest", "reexports": [],
        "crateReexports": ["AssetNaming", "NAMES", "verify_directory"],
        "graph": graph, "overlay": pin(PORTABLE, PORTABLE / SCOPE / "rust.json"),
    })
    for current, slot in ((native, f"{SCOPE}/Cargo.lock"),
                          (workspace, f"{Path(OUTPUT).stem}/Cargo.lock")):
        source = current["cargoLock"]
        previous = check_pin(PORTABLE / source["path"], source)
        current["cargoLock"] = emit(slot, add_fixture_dependencies(previous, upstream_lock))
    native["devDependencies"] = {"tempfile": {"version": "3.27.0"}}
    workspace["members"][MEMBER]["manifest"] = emit(PACKAGE, encode(native))
    emit(OUTPUT, encode(workspace))


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("original", type=Path)
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()
    compose(args.original, args.check)
