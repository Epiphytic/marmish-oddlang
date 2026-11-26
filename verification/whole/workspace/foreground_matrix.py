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
    parser.add_argument("--both-initializers", action="store_true")
    parser.add_argument("--provider-terminal", action="store_true")
    args = parser.parse_args()
    binary = args.workspace / "target/release"
    with tempfile.TemporaryDirectory(prefix="mv4-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        if args.both_initializers:
            result = {}
            for name, initializer in (("originalInit", args.original), ("generatedInit", binary)):
                case = root / name
                case.mkdir()
                result[name] = observe(args.original, binary, case, trust_failure=True,
                                       include_generated_client=True, profile_initializer=initializer / "marmish",
                                       provider_terminal=args.provider_terminal)
            if result["originalInit"] != result["generatedInit"]:
                raise ValueError("foreground observations differ by profile initializer")
        else:
            result = observe(args.original, binary, root, trust_failure=True,
                             include_generated_client=True, provider_terminal=args.provider_terminal)
    result["generatedBinarySha256"] = {
        name: hashlib.sha256((binary / name).read_bytes()).hexdigest()
        for name in ("marmish", "marmishd")
    }
    with args.receipt.open("x") as stream:
        stream.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
