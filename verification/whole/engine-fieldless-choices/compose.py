"""Recover the two reviewed source spans and pin the replacement engine member."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
DOMAIN = "engine-fieldless-choices-v1"
BASE = "engine-observational-records-package.json"
PACKAGE = "engine-fieldless-choices-package.json"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = Path("crates/marmish-engine/src/model.rs")
SOURCE_SHA256 = "f147e2d08551d7d758a89b46857805820567a78f70571a663f33ea91672c9d53"
TYPES = {"Stream": "stream", "SubmitDisposition": "submit-disposition"}
IDENTITY = "urn:marmish:engine:fieldless-choices:v1"


def sha(content):
    return hashlib.sha256(content).hexdigest()


def encode(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def load(path):
    return json.loads(path.read_bytes())


def original_source(root):
    source = subprocess.check_output(["git", "-C", root, "show", f"{ORIGINAL}:{SOURCE.as_posix()}"])
    if sha(source) != SOURCE_SHA256:
        raise ValueError("original engine model pin differs")
    return source


def declaration(model, name):
    marker = rb"^#\[derive\([^\r\n]+\)\]\npub enum " + name.encode() + rb" \{\n[^}]*\}\n"
    matches = re.findall(marker, model, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError(f"expected a unique reviewed span for {name}")
    return matches[0]


def mutated(model):
    for name, old, new in (
        ("Stream", b"    Stdout,\n    Stderr,", b"    Stderr,\n    Stdout,"),
        ("SubmitDisposition", b"#[derive(Clone, Debug,", b"#[derive(Clone, Copy, Debug,"),
        ("SubmitDisposition", b"    Accepted,\n    Existing,", b"    Existing,\n    Accepted,"),
    ):
        span = declaration(model, name)
        if span.count(old) != 1:
            raise ValueError(f"mutation does not match {name}")
        model = model.replace(span, span.replace(old, new), 1)
    return model


def recover(compiler, model, work):
    types, names, variants, evidence = {}, {}, {}, {}
    for name, suffix in TYPES.items():
        work.mkdir(exist_ok=True, parents=True)
        span = declaration(model, name)
        source, selection, output = [work / f"{suffix}{ext}" for ext in (".rs", ".json", "")]
        source.write_bytes(span)
        selection.write_bytes(encode({"format": "oddlang-plain-choice-selection-v2", "id": f"{IDENTITY}:{suffix}",
                                       "name": name, "sourceSha256": sha(span)}))
        subprocess.run([compiler, "recover", "plain-choice", source, selection, output], check=True)
        types.update(load(output / "graph.json")["types"])
        binding = load(output / "rust.json")
        names.update(binding["types"])
        variants.update(binding["variants"])
        evidence[name] = load(output / "observation.json")
    graph = encode({"format": "oddlang-checked-values-v1", "id": IDENTITY, "types": types})
    binding = encode({"format": "oddlang-checked-values-rust-v1", "types": names,
                      "variants": variants, "errors": {}})
    return graph, binding, evidence


def descriptor(rel, content):
    return {"path": rel, "sha256": sha(content)}


def package(graph, binding):
    result = load(PORTABLE / BASE)
    result["components"].append({
        "kind": "checked-values", "module": "model", "reexports": list(TYPES),
        "graph": descriptor(f"{DOMAIN}/graph.json", graph),
        "overlay": descriptor(f"{DOMAIN}/rust.json", binding),
    })
    return (json.dumps(result, indent=2) + "\n").encode()


def compose(compiler, original, check):
    with tempfile.TemporaryDirectory(prefix="engine-choice-recover-") as temp:
        graph, binding, _ = recover(compiler, original_source(original), Path(temp))
    for path, content in ((PORTABLE / DOMAIN / "graph.json", graph),
                          (PORTABLE / DOMAIN / "rust.json", binding),
                          (PORTABLE / PACKAGE, package(graph, binding))):
        if check:
            if path.read_bytes() != content:
                raise ValueError(f"sealed package differs: {path}")
        else:
            path.write_bytes(content)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("compiler", type=lambda p: Path(p).resolve())
    cli.add_argument("original", type=Path)
    cli.add_argument("--write", action="store_true")
    args = cli.parse_args()
    compose(args.compiler, args.original, check=not args.write)
