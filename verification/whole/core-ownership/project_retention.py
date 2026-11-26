"""Select a portable type by identity and replay its existing TypeScript qualification."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from rdflib import Graph, Literal

from semantic_roles import HERE, M, O, PIN, PORTABLE, remove_native_bindings, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    args = parser.parse_args()
    graph = Graph().parse(PORTABLE / "core-structure-v1/ownership.ttl")
    remove_native_bindings(graph)
    rows = graph.query((HERE / "semantic-roles.sparql.txt").read_text())
    row, = (row for row in rows if row.type == M["retention-policy"])
    require(row.role == O.domain and str(row.source).startswith(PIN), "expected a pinned domain")
    domain = (PORTABLE / str(row.source).removeprefix(PIN)).resolve()
    require(domain.is_relative_to(PORTABLE.resolve()), "domain escapes portable inputs")
    domain_bytes = domain.read_bytes()
    require(row.digest == Literal(hashlib.sha256(domain_bytes).hexdigest()), "wrong domain digest")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        inputs, output = root / "input", root / "output"
        inputs.mkdir()
        (inputs / "domain.ttl").write_bytes(domain_bytes)
        subprocess.run([args.oddlang.resolve(), "build", "value-domain", "typescript", inputs, output],
                       check=True, cwd=root)
        require((output / "projection.lock").read_bytes() == (domain.parent / "typescript.lock").read_bytes(),
                "TypeScript projection lock differs")
        lock = json.loads((output / "projection.lock").read_bytes())
        require(lock["bindingSha256"] is None and lock["artifact"] == "value.mts", "unexpected target binding")
        for name, key in (("value.mts", "artifactSha256"), ("obligations.json", "obligationsSha256")):
            require(hashlib.sha256((output / name).read_bytes()).hexdigest() == lock[key],
                    f"emitted bytes differ from lock for {name}")
        result = subprocess.check_output(["node", "--experimental-strip-types", HERE.parent / "core-node/check.mjs", output],
                                         cwd=root, text=True)
        print(json.dumps({"type": str(row.type), "graphSha256": str(row.digest), "node": json.loads(result)}, sort_keys=True))


if __name__ == "__main__":
    main()
