"""Compare pinned native binaries using only the fixed parse-termination corpus."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import runpy
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
BUILD = runpy.run_path(str(HERE / "build.py"))
BASE, WORKSPACE = BUILD["BASE"], BUILD["WORKSPACE"]
SEVEN = HERE.parent / "unified-buffered/aarch64-apple-darwin.json"
CLI = HERE.parent / "cli/transcript.json"
SUCCESSOR_FRAMEWORK = "0fc48e920e30baf6396218cee0983732ba6c3a78"
ROLES = ("marmish", "marmishd")
CASES = [
    ["-h"], ["-V"], ["help"], ["help", "--help"], ["help", "not-a-command"],
    ["put"], ["get", "one", "two"], ["stream"], ["stream-reconcile"],
    ["setup", "--role=unknown"], ["install"], ["install-status"], ["uninstall"],
    ["cancel"], ["wait"], ["output"], ["command-status"],
    ["--stdin", "--stdin-none", "status"], ["--wait-seconds=0", "status"],
    ["--wait-seconds=86401", "status"], ["--key-storage=unknown", "status"],
    ["put", "--idle-seconds=0"], ["copy-receipts", "--forget=x"],
    ["copy-receipts", "--revision=0"], ["status", "--unknown"], ["status", "--version"],
    ["rsh", "--help"], ["rsh", "-h"], ["rsh"], ["rsh", "--idle-seconds=0"],
    ["--debug", "rsh", "--help"], ["--user=audit", "rsh", "--help"],
]
POLICY = "(version 1) (allow default) (deny network*) (deny file-write*) (deny process-fork)"


def sha(path):
    with path.open("rb") as data:
        return hashlib.file_digest(data, "sha256").hexdigest()


def observe(path, args, home):
    result = subprocess.run(["/usr/bin/sandbox-exec", "-p", POLICY, path, *args],
                            cwd=home, env={"HOME": str(home), "LANG": "C", "PATH": "/usr/bin:/bin"},
                            input=b"", capture_output=True, timeout=5, umask=0o077)
    return {"args": args, "exit": result.returncode, "stdout": result.stdout.decode("ascii"),
            "stderr": result.stderr.decode("ascii")}


def compare(original, baseline, baseline_build, successor, successor_build):
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        raise ValueError("this audit uses the native Darwin ARM64 release artifacts")
    directories = {"original": original, "baseline": baseline, "successor": successor}
    identities = {side: {name: sha(path / name) for name in ROLES}
                  for side, path in directories.items()}
    builds = {}
    for name, build_receipt, manifest in (
        ("baseline", baseline_build, BASE), ("successor", successor_build, WORKSPACE),
    ):
        build = json.loads(build_receipt.read_bytes())
        if (build["binarySha256"] != identities[name]
                or build["workspaceManifestSha256"] != sha(manifest)
                or build["workspaceLockSha256"] != sha(manifest.with_suffix("") / "workspace.lock")):
            raise ValueError(f"generated binary or workspace pin differs: {name}")
        builds[name] = build
    provenance = json.loads(SEVEN.read_bytes())
    if builds["baseline"]["frameworkRevision"] != provenance["framework"]:
        raise ValueError("baseline compiler revision differs from the sealed release receipt")
    if builds["successor"]["frameworkRevision"] != SUCCESSOR_FRAMEWORK:
        raise ValueError("successor compiler revision differs from the reviewed runtime")
    if identities["original"] != provenance["releaseInfo"]["originalBinaries"]:
        raise ValueError("original binaries differ from the sealed release receipt")
    receipts = json.loads(CLI.read_bytes())
    observed = {}
    with tempfile.TemporaryDirectory(prefix="offline-cli-audit-") as temp:
        home = Path(temp).resolve()
        for name in ROLES:
            rows = {}
            for side, directory in directories.items():
                exe = (directory / name).resolve()
                prior = []
                for row in receipts[name]:
                    args = row["args"]
                    if not (args in (["--help"], ["--version"], ["--oddlang-unrecognized-option"])
                            or len(args) == 2 and args[0] == "help"
                            and args[1].replace("-", "").isascii() and args[1].replace("-", "").isalnum()):
                        raise ValueError(f"unexpected historical invocation: {args!r}")
                    prior.append(observe(exe, args, home))
                if prior != receipts[name]:
                    raise ValueError(f"changed historical transcript in {side}/{name}")
                rows[side] = [observe(exe, args, home) for args in CASES]
            observed[name] = [
                {"original": original_row, **{side: rows[side][index] for side in directories if side != "original"}}
                for index, original_row in enumerate(rows["original"])
            ]
        if any(home.iterdir()):
            raise ValueError("parse-only audit changed working directory")
    for name, rows in observed.items():
        baseline_gaps = [row["original"]["args"] for row in rows if row["baseline"] != row["original"]]
        successor_gaps = [row["original"]["args"] for row in rows if row["successor"] != row["original"]]
        if baseline_gaps != CASES[-6:] or successor_gaps != CASES[-2:]:
            raise ValueError(f"unexpected parser compatibility results for {name}")
    return {"platform": provenance["platform"], "originalRevision": provenance["original"],
            "baselineInputRevision": provenance["companion"], "baselineFramework": provenance["framework"],
            "baselineWorkspaceLockSha256": provenance["workspaceLockSha256"],
            "originalReleaseReceiptSha256": sha(SEVEN), "historicalTranscriptSha256": sha(CLI),
            "historicalRowsPerBinary": {name: len(rows) for name, rows in receipts.items()},
            "binarySha256": identities, "builds": builds, "rows": observed}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("original", type=Path)
    cli.add_argument("baseline", type=Path)
    cli.add_argument("output", type=Path)
    cli.add_argument("baseline_build", type=Path)
    cli.add_argument("successor", type=Path)
    cli.add_argument("successor_build", type=Path)
    args = cli.parse_args()
    receipt = compare(args.original, args.baseline, args.baseline_build, args.successor, args.successor_build)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n")
    for name, rows in receipt["rows"].items():
        print(name, {side: sum(row[side] == row["original"] for row in rows)
                    for side in receipt["binarySha256"] if side != "original"}, "/", len(rows))
