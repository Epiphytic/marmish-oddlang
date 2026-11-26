"""Check the pinned reference relay on loopback with its independent test clients."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

PIN = "b38b2841b081952dc7728da391fe0e26fc7c6181"
CASES = [
    "challenges_first_and_finishes_accepted_publish_after_sender_disconnect",
    "challenge_precedes_a_pipelined_client_request",
    "cli_reports_actual_bind_and_structured_logs_redact_wire_data",
]


def replay(root, target):
    def run(*args):
        return subprocess.check_output(args, cwd=root, text=True).strip()

    if run("git", "rev-parse", "HEAD") != PIN or run("git", "status", "--porcelain"):
        raise ValueError("reference relay checkout must be clean and pinned")
    results = []
    for name in CASES:
        result = run("cargo", "+1.97.1", "test", "--release", "--locked", "--offline",
                     "--target-dir", str(target), "-p", "deaddrop-server", "--test", "debug_ws",
                     "--", name, "--exact", "--test-threads=1")
        if f"test {name} ... ok" not in result or "1 passed; 0 failed;" not in result:
            raise ValueError(f"missing exact passing test: {name}")
        results.append(name)
    return {"commit": PIN, "toolchain": run("rustc", "+1.97.1", "-vV"),
            "lockSha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
            "profile": "release", "transport": "loopback websocket", "passed": results}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("checkout", type=Path)
    p.add_argument("target_dir", type=Path)
    args = p.parse_args()
    print(json.dumps(replay(args.checkout.resolve(), args.target_dir.resolve()), indent=2))
