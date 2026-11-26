"""Select a portable type by identity and replay its existing TypeScript qualification."""

import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from rdflib import Graph

from semantic_roles import HERE, M, PIN, PORTABLE, remove_native_bindings, require, resolve


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oddlang", type=Path)
    args = parser.parse_args()
    graph = Graph().parse(PORTABLE / "core-structure-v1/ownership.ttl")
    remove_native_bindings(graph)
    row, = (row for row in resolve(graph) if row.type == M["retention-policy"])
    domain = PORTABLE / str(row.source).removeprefix(PIN)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        inputs, output = root / "input", root / "output"
        inputs.mkdir()
        shutil.copyfile(domain, inputs / "domain.ttl")
        subprocess.run([args.oddlang.resolve(), "build", "value-domain", "typescript", inputs, output],
                       check=True, cwd=root)
        require((output / "projection.lock").read_bytes() == (domain.parent / "typescript.lock").read_bytes(),
                "TypeScript projection lock differs")
        result = subprocess.check_output(["node", "--experimental-strip-types", HERE.parent / "core-node/check.mjs", output],
                                         cwd=root, text=True)
        print(json.dumps({"type": str(row.type), "graphSha256": str(row.digest), "node": json.loads(result)}, sort_keys=True))


if __name__ == "__main__":
    main()
