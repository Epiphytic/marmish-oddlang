"""Capture the local legacy version-probe boundary using independently built fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tempfile

PIN = "37c248a11239e108afc9bd4ee62b8f60542012e3"
LOCK = "d8a5367e49427c79f354b0611902808de9b92abbe173eb82ed6864d01e488a71"


def sha(file):
    with file.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def run(args):
    def git(*argv):
        return subprocess.check_output(["git", "-C", args.wn_source, *argv], text=True).strip()

    if git("rev-parse", "HEAD") != PIN or git("status", "--porcelain") or sha(args.wn_source / "Cargo.lock") != LOCK:
        raise ValueError("unexpected wn build source")
    # Build from the known source; the wn binary from a historical installed pair has separate provenance.
    subprocess.run(["cargo", "+1.97.1", "build", "--locked", "--release", "--offline",
                    "-p", "wn-cli", "--bin", "wn", "--target-dir", args.wn_target], cwd=args.wn_source, check=True)
    wn = args.wn_target.resolve() / "release/wn"
    receipt = json.loads(Path(str(args.wnd) + ".manifest.json").read_bytes())
    if (receipt["revision"], receipt["cargo_lock_sha256"], receipt["sha256"]) != (PIN, LOCK, sha(args.wnd)):
        raise ValueError("wnd provenance mismatch")
    native = json.loads(args.build_receipt.read_bytes())
    if native["builds"]["mdk"]["sha256"]["examples/probe"] != sha(args.probe):
        raise ValueError("original probe hash mismatch")
    with tempfile.TemporaryDirectory() as tmp:
        observed = subprocess.run([args.probe.resolve(), wn], cwd=tmp, env={}, stdin=subprocess.DEVNULL,
                                  capture_output=True, timeout=8, check=True)
    if observed.stdout != b"Unverified binary observation; no runtime provenance or live egress approval.\nwn 0.10.3\n" or observed.stderr:
        raise ValueError(f"unexpected version probe: {(observed.stdout, observed.stderr)!r}")
    return {"upstreamRevision": native["commit"], "host": platform.platform(), "fixtureRevision": PIN,
            "wnBinarySha256": sha(wn), "probeSha256": sha(args.probe), "wnLockSha256": LOCK,
            "versionStdout": observed.stdout.decode(), "versionStderr": observed.stderr.decode(),
            "wndBuild": receipt}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ["wn_source", "wn_target", "wnd", "probe", "build_receipt"]:
        p.add_argument(name, type=Path)
    print(json.dumps(run(p.parse_args()), indent=2, sort_keys=True))
