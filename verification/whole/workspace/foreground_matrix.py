"""Exercise all four real foreground status pairings from one composed workspace."""

import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "foreground-owner"))
from qualify import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("original", "workspace", "receipt"):
        parser.add_argument(arg, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    binary = args.workspace / "target/release"
    with tempfile.TemporaryDirectory(prefix="mv4-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        result = observe(args.original, binary, root, trust_failure=True, include_generated_client=True)
    result["generatedBinarySha256"] = {
        name: hashlib.sha256((binary / name).read_bytes()).hexdigest()
        for name in ("marmish", "marmishd")
    }
    with args.receipt.open("x") as stream:
        stream.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
