"""Recover the three original structural baselines from a pinned checkout."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import hashlib
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from framework_pin import build_cli

SOURCE = "af54339d250290490d62d4475a77539b7e739071"
CASES = [
    ("marmish-stream", "lib", "marmish_stream"),
    ("marmish-prototype", "lib", "marmish_prototype"),
    ("marmish-prototype", "bin", "marmish-engine-bridge"),
]


def recover(args, oddlang):
    expected = Path(__file__).resolve().parent / args.triple
    if not expected.is_dir():
        raise ValueError(f"missing archived structural baseline for {args.triple}")
    profile_dir = args.framework / "examples/marmish/portable-contract"
    base = (profile_dir / "structure.toml").read_text()
    adapter = (args.framework / "verification/rust-recovery/run.sh").resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    for package, kind, target in CASES:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory) / "profile.toml"
            lines = {
                "sourceRevision": SOURCE,
                "adapter": str(adapter),
                "package": package,
                "targetKind": kind,
                "targetName": target,
                "targetTriple": args.triple,
            }
            profile.write_text("\n".join(
                f"{key} = {json.dumps(lines[key])}" if key in lines else line
                for line in base.splitlines()
                for key in [line.partition("=")[0].strip()]
            ) + "\n")
            subprocess.run([
                oddlang, "recover", "rust", str(args.upstream / "Cargo.toml"),
                "--profile", str(profile), "--out", str(args.output / target),
            ], check=True)
            actual = args.output / target
            lock = json.loads((actual / "structure.lock").read_bytes())
            if lock != json.loads((expected / f"{target}.lock.json").read_bytes()):
                raise ValueError(f"{target} differs from the archived structural lock")
            digest = hashlib.sha256((actual / "structure.ttl").read_bytes()).hexdigest()
            if digest != lock["structureSha256"]:
                raise ValueError(f"{target} graph digest mismatch")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ["framework", "upstream", "output"]:
        p.add_argument(name, type=Path)
    p.add_argument("--triple", required=True)
    args = p.parse_args()
    with build_cli(args.framework.resolve()) as oddlang:
        recover(args, oddlang)
