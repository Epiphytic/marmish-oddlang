"""Qualify deep local reply variants against an independently witnessed original CLI."""

import argparse
import hashlib
import json
from pathlib import Path

from observe import expect
from reply_shapes import replay

CASES = Path(__file__).with_name("typed-reply-variants.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("receipt", type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--reference-status", type=Path)
    mode.add_argument("--compare", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    cases = CASES.read_bytes()
    case_digest = hashlib.sha256(cases).hexdigest()
    reference = json.loads((args.reference_status or args.compare).read_bytes())
    expect(reference["mode"], "original", "reference provenance")
    if args.reference_status:
        expect(digest, reference["originalBinaries"]["marmish"], "original binary digest")
    else:
        expect(case_digest, reference["caseSha256"], "corpus digest")
    results = replay(binary, json.loads(cases))
    data = {
        "binarySha256": digest,
        "caseSha256": case_digest,
        "mode": "candidate" if args.compare else "original",
        "originalRevision": reference["originalRevision"],
        "results": results,
    }
    if args.compare:
        expected = reference["results"]
        expect(set(expected), set(results), "case names")
        data["mismatches"] = sorted(case for case in expected if results[case] != expected[case])
    args.receipt.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    if data.get("mismatches"):
        raise SystemExit("different replies: " + ", ".join(data["mismatches"]))
    print(f"Observed {len(results)} typed local reply variants")


if __name__ == "__main__":
    main()
