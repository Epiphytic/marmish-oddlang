"""Replay the exact fixed cases directly before using a remote peer."""
import argparse
from dataclasses import replace
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from corpus import SHORT_CASES, digest, expected_receipt, finite_case, verify_output


def replay_case(case, executable, args, root):
    result = subprocess.run(
        [os.fsencode(executable), *args],
        input=case.stdin,
        capture_output=True,
        cwd=root,
        env={"HOME": str(root), "TMPDIR": str(root), "PATH": "/usr/bin:/bin", "LC_ALL": "C"},
        timeout=330,
        umask=0o077,
    )
    verify_output(case, replace(case, status=result.returncode, stdout=result.stdout, stderr=result.stderr))
    nonce = args[0 if case.name == "finite" else 1].decode()
    receipt = json.loads((root / "receipts" / f"{nonce}.receipt").read_bytes())
    if receipt != expected_receipt(case, executable, args):
        raise ValueError(f"{case.name}: fixture receipt mismatch")
    # The argv digest includes a temporary absolute path. Nonces identify private receipts.
    return {key: value for key, value in receipt.items() if key not in {"argv_sha256", "nonce"}}


def replay(upstream_fixture, upstream_fixture_digest):
    build = json.loads(Path(__file__).with_name("reference-build-macos-arm64.json").read_bytes())
    if (upstream_fixture_digest != build["fixture"]["sha256"]
            or digest(upstream_fixture.read_bytes()) != upstream_fixture_digest):
        raise ValueError("upstream fixture digest mismatch")
    results = []
    with tempfile.TemporaryDirectory(prefix="od-corpus-", dir="/tmp") as directory:
        root = Path(directory).resolve(strict=True)
        (root / "receipts").mkdir(mode=0o700)
        short = root / "short"
        shutil.copyfile(Path(__file__).with_name("corpus.py"), short)
        short.chmod(0o500)
        finite = root / "finite"
        shutil.copyfile(upstream_fixture, finite)
        finite.chmod(0o500)
        if digest(finite.read_bytes()) != upstream_fixture_digest:
            raise ValueError("copied upstream fixture digest mismatch")
        for case in SHORT_CASES:
            results.append(replay_case(case, short, (case.name.encode(), digest(case.name.encode()).encode()), root))
        for index in range(4):
            case = finite_case(index)
            results.append(dict(replay_case(case, finite, case.args, root), repetition=index))
    if root.exists():
        raise ValueError("fixture workspace cleanup failed")
    return {"schema": 1, "cases": results, "workspace_removed": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream_fixture", type=Path)
    parser.add_argument("upstream_fixture_sha256")
    args = parser.parse_args()
    print(json.dumps(replay(args.upstream_fixture, args.upstream_fixture_sha256), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
