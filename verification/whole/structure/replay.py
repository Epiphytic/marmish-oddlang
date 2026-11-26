"""Recover the three original structural baselines from a pinned checkout."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

SOURCE = "af54339d250290490d62d4475a77539b7e739071"
CASES = [
    ("marmish-stream", "lib", "marmish_stream"),
    ("marmish-prototype", "lib", "marmish_prototype"),
    ("marmish-prototype", "bin", "marmish-engine-bridge"),
]


def recover(args):
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
                args.oddlang, "recover", "rust", str(args.upstream / "Cargo.toml"),
                "--profile", str(profile), "--out", str(args.output / target),
            ], check=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ["framework", "upstream", "output"]:
        p.add_argument(name, type=Path)
    p.add_argument("--oddlang", default="oddlang")
    p.add_argument("--triple", required=True)
    recover(p.parse_args())
