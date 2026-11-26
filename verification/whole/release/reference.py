"""Compare the selected source with the independently exercised v0.3.1 release."""
import argparse
import hashlib
import json
import subprocess

PIN = "af54339d250290490d62d4475a77539b7e739071"
RELEASE = "b666ae6aaa6063b1b30317d43fdb86bca3e4ec95"


def reference(checkout):
    def git(*args):
        return subprocess.check_output(["git", "-C", checkout, *args])

    unchanged = {}
    for path in ["Cargo.toml", "Cargo.lock", "crates", "patches", "rust-toolchain.toml"]:
        before, after = [
            git("rev-parse", f"{rev}:{path}").decode().strip() for rev in (RELEASE, PIN)
        ]
        if before != after:
            raise ValueError(f"release build input changed: {path}")
        unchanged[path] = after
    raw = git("show", f"{PIN}:docs/validation/v0.3.1-measurements.json")
    report = json.loads(raw)
    if report["source"] != RELEASE:
        raise ValueError("measurement provenance mismatch")
    return {
        "selectedCommit": PIN,
        "measuredCommit": RELEASE,
        "sharedBuildInputs": unchanged,
        "changedPaths": git("diff", "--name-only", RELEASE, PIN).decode().splitlines(),
        "measurementSha256": hashlib.sha256(raw).hexdigest(),
        "candidateRun": report["candidate_run"],
        "qualificationRun": report["qualification_run"],
        "transferCases": {c["cohort"]: c["passing_cases"] for c in report["transfer_cohorts"]},
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout")
    print(json.dumps(reference(parser.parse_args().checkout), indent=2, sort_keys=True))
