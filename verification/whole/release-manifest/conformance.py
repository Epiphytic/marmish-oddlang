"""Export portable manifest cases and check the rows returned by a consumer."""
import argparse
import json
from pathlib import Path

from corpus import cases


def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError(f"duplicate JSON member {key!r}")
        obj[key] = value
    return obj


def encode(value):
    # JSON spelling distinguishes boolean results from 0 and 1.
    return json.dumps(value, ensure_ascii=True, sort_keys=True, allow_nan=False)


def check(rows, fixtures):
    if not isinstance(rows, list) or len(rows) != len(fixtures):
        raise ValueError(f"expected {len(fixtures)} ordered result rows")
    for row, (name, (_, expected)) in zip(rows, fixtures.items()):
        if encode(row) != encode({"case": name, "result": expected}):
            raise ValueError(f"result mismatch at {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("inputs", "vectors", "check"):
        commands.add_parser(command).add_argument("path", type=Path)
    args = parser.parse_args()
    fixtures = cases()
    if args.command == "check":
        try:
            rows = json.loads(args.path.read_bytes(), object_pairs_hook=unique_object)
            check(rows, fixtures)
        except ValueError as exc:
            parser.exit(1, f"{exc}\n")
        print(f"{len(rows)} manifest results match")
        return
    rows = []
    for name, (raw, expected) in fixtures.items():
        row = {"case": name, "hex": raw.hex()}
        if args.command == "vectors":
            row["result"] = expected
        rows.append(row)
    document = "[\n" + ",\n".join(encode(row) for row in rows) + "\n]\n"
    args.path.write_bytes(document.encode("ascii"))


if __name__ == "__main__":
    main()
