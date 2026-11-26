"""Bind an original workspace test log to the enumerated release harnesses used by the run."""
import argparse
import hashlib
import json
from pathlib import Path
import re

HEADER = re.compile(r"^\s*Running .+ \((.+)\)$")
DOCS = re.compile(r"^\s*Doc-tests (\S+)\s*$")
RESULT = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;")


def file_hash(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def summarize(log, inventory):
    baseline = json.loads(inventory.read_bytes())
    keys = {v["binarySha256"]: key for key,v in baseline["harnesses"].items()}
    if len(keys) != len(baseline["harnesses"]):
        raise ValueError("named inventory has ambiguous harness hashes")
    binaries = {}
    doctests = {}
    section = None
    last = None

    def finish():
        if section is None:
            return
        table, key = section
        if key in table or last is None:
            raise ValueError(f"duplicate or unfinished test section {key}")
        outcome, passed, failed, ignored, measured, filtered = last
        if outcome != "ok" or failed or filtered or measured:
            raise ValueError(f"incomplete release run for {key}: {last}")
        if table is binaries:
            row = baseline["harnesses"][key]
            if passed != len(row["tests"]) or ignored != len(row["ignored"]):
                raise ValueError(f"executed and named counts disagree for {key}")
        table[key] = {"passed": passed, "ignored": ignored}

    for line in log.read_text().splitlines():
        if match := HEADER.match(line):
            finish()
            path = Path(match[1])
            if not path.is_absolute():
                raise ValueError("run Cargo with an absolute target directory")
            section, last = (binaries, keys[file_hash(path)]), None
        elif match := DOCS.match(line):
            finish()
            section, last = (doctests, match[1]), None
        elif section and (match := RESULT.match(line)):
            # A harness may recursively start itself, producing nested summaries.
            # Cargo's outer summary is the last one before its next target.
            last = (match[1], *map(int, match.groups()[1:]))
    finish()
    if binaries.keys() != baseline["harnesses"].keys():
        raise ValueError("missing compiled release harnesses")
    libraries = {name.rsplit("/", 1)[-1] for name in binaries if "/lib/" in name}
    if doctests.keys() != libraries:
        raise ValueError(f"unexpected doctest targets {set(doctests) ^ libraries}")
    return {"upstream":baseline["upstream"], "namedInventorySha256":file_hash(inventory),
            "logSha256":file_hash(log), "selection":baseline["selection"],
            "harnesses":dict(sorted(binaries.items())), "doctests":dict(sorted(doctests.items()))}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("log", "named_inventory", "output"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(summarize(args.log, args.named_inventory), indent=2) + "\n")
