"""Check the original or a replacement against the frozen CLI parse-only transcript."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
HIDDEN = ["command-evidence", "command-cleanup-status", "command-local-cancel", "command-prepare-cleanup"]
ROLES = ["marmish", "marmishd"]


def observe(binary, args, cwd):
    result = subprocess.run([binary, *args], cwd=cwd, input=b"", capture_output=True, timeout=8,
                            env={"PATH":"/usr/bin:/bin", "LANG":"C","HOME":str(cwd)}, umask=0o077)
    return {"args": args, "exit": result.returncode, "stdout": result.stdout.decode("ascii"),
            "stderr": result.stderr.decode("ascii")}


def cases(binary, cwd):
    root = observe(binary, ["--help"], cwd)
    section = root["stdout"].partition("\nCommands:\n")[2].partition("\n\n")[0]
    commands = re.findall(r"^  ([\w-]+)\s", section, re.MULTILINE)
    if root["exit"] or len(commands) != len(set(commands)) or not commands or "help" not in commands:
        raise ValueError("cannot enumerate root help")
    rows = [root]
    for command in [*commands, *HIDDEN]:
        row = observe(binary, ["help", command], cwd)
        if row["exit"] or not row["stdout"] or row["stderr"]:
            raise ValueError(f"{binary.name} failed to describe {command}")
        rows.append(row)
    rows.append(observe(binary, ["--version"], cwd))
    rows.append(observe(binary, ["--oddlang-unrecognized-option"], cwd))
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode", choices=["capture", "verify"])
    p.add_argument("bindir", type=Path)
    p.add_argument("--fixture", type=Path, default=HERE / "transcript.json")
    p.add_argument("--build-receipt", type=Path, default=HERE.parent / "bridge/aarch64-apple-darwin.json")
    args = p.parse_args()
    if args.mode == "capture":
        pins = json.loads(args.build_receipt.read_bytes())["builds"]["default"]["sha256"]
        for role in ROLES:
            if hashlib.sha256((args.bindir / role).read_bytes()).hexdigest() != pins[role]:
                raise ValueError(f"unrecognized original binary: {role}")
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        observed = {role: cases((args.bindir / role).resolve(), root) for role in ROLES}
        if any(root.iterdir()):
            raise ValueError("CLI parse-only run modified its home")
    if args.mode == "capture":
        with args.fixture.open("x") as f:
            f.write(json.dumps(observed, indent=2) + "\n")
    else:
        expected = json.loads(args.fixture.read_bytes())
        if observed != expected:
            for role in ROLES:
                for row in expected[role]:
                    if row not in observed[role]:
                        raise ValueError(f"changed response: {role} {row['args']}")
            raise ValueError("unexpected additional CLI commands or changed ordering")
    print({role: len(rows) for role, rows in observed.items()})


if __name__ == "__main__":
    main()
