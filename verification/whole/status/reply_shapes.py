"""Replay the extended local reply contract through a status CLI on a Unix socket."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import tempfile

from observe import expect, fake_peer, frame

HERE = Path(__file__).resolve().parent


def replay(binary, cases):
    observations = {}
    with tempfile.TemporaryDirectory(prefix="odshape-", dir="/tmp") as workdir:
        root = Path(workdir)
        state = root / "state"
        state.mkdir(mode=0o700)
        path = root / "s"
        env = {"HOME": workdir, "TMPDIR": workdir, "LANG": "C", "PATH": os.defpath}
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(path))
            listener.listen()
            listener.settimeout(4)
            for case, payload in cases.items():
                wire = frame(json.dumps(payload, separators=(",", ":")).encode())
                observations[case] = fake_peer(binary, listener, state, path, env, wire)
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("receipt", type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--reference-status", type=Path, help="Verify original CLI SHA-256 against an original platform status receipt")
    mode.add_argument("--compare", type=Path, help="Compare a candidate against a recorded original reply-shapes receipt")
    args = parser.parse_args()
    binary = args.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    case_bytes = (HERE / "reply-shapes.json").read_bytes()
    case_digest = hashlib.sha256(case_bytes).hexdigest()
    if args.reference_status:
        status = json.loads(args.reference_status.read_bytes())
        expect(status["mode"], "original", "original baseline")
        expect(digest, status["originalBinaries"]["marmish"], "original CLI")
        revision = status["originalRevision"]
    else:
        reference = json.loads(args.compare.read_bytes())
        expect(reference["mode"], "original", "reference provenance")
        expect(reference["caseSha256"], case_digest, "case corpus")
        revision = reference["originalRevision"]
    results = replay(binary, json.loads(case_bytes))
    data = {
        "binarySha256": digest,
        "caseSha256": case_digest,
        "mode": "original" if args.reference_status else "candidate",
        "originalRevision": revision,
        "results": results,
    }
    if args.compare:
        expected = reference["results"]
        expect(set(expected), set(results), "case names")
        data["mismatches"] = sorted(case for case in expected if results[case] != expected[case])
    args.receipt.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    if data.get("mismatches"):
        raise SystemExit("different replies: " + ", ".join(data["mismatches"]))


if __name__ == "__main__":
    main()
