#!/usr/bin/env python3
"""Build original release binaries and replay pinned external bridge observations."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import sys
import tempfile

PIN = "af54339d250290490d62d4475a77539b7e739071"
TREE = "9be3104dd1d55194ae6e442c0d3eb1c0b223a481"
CARGO_LOCK = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3"
DRIVER = "7a675cffc8b08d5773645434df14aed77bfbf9c437e4c3044dc8fc1f9ced628a"
FIXTURE = "08dd1637bc59b9b17571f269d538b8ef07e1ed931f8a78e558b2c88e60ee7272"
HIGH_NOFILE = 1_048_576
HOSTS = {"aarch64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"}
DEFAULT_BINS = [
    "marmish", "marmishd", "marmish-worker", "marmish-release-tool",
    "marmish-engine-worker", "marmish-engine-bridge", "marmish-keychain",
]
BUILDS = [
    ("default", ["--workspace", "--bins"], DEFAULT_BINS),
    ("mdk", ["-p", "marmish-mdk", "--example", "probe"], ["examples/probe"]),
    ("fault", ["-p", "marmish-engine", "--features", "fault-injection", "--bin",
               "marmish-test-frontend"], ["marmish-test-frontend"]),
    ("installed", ["-p", "marmish-native", "--features", "installed-checks", "--bin",
                   "marmish-installed-check", "--bin", "marmish-installed-fixture"],
     ["marmish-installed-check", "marmish-installed-fixture"]),
]


def sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def check(actual, expected, label):
    if actual != expected:
        raise RuntimeError(f"{label}: expected {expected!r}, observed {actual!r}")


def output(cmd, cwd=None):
    return subprocess.check_output(cmd, cwd=cwd, text=True).strip()


def verify_source(root):
    for args, expected in [(["rev-parse", "HEAD"], PIN), (["rev-parse", "HEAD^{tree}"], TREE),
                           (["status", "--porcelain=v1", "--untracked-files=all"], "")]:
        check(output(["git", *args], cwd=root), expected, "upstream checkout")
    check(sha(root / "Cargo.lock"), CARGO_LOCK, "upstream lock")


def load_driver(oddlang):
    path = oddlang / "examples/marmish/portable-bridge/verify.py"
    check(sha(path), DRIVER, "oracle driver")
    check(sha(path.with_name("baseline.json")), FIXTURE, "oracle fixture")
    spec = importlib.util.spec_from_file_location("pinned_bridge", path)
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    return driver


def replay(driver, bins):
    with tempfile.TemporaryDirectory() as temp:
        out = Path(temp) / "observations.json"
        subprocess.run([sys.executable, "-B", driver.__file__,
                        bins / "marmish-engine-bridge", bins / "marmish-engine-worker",
                        "--report", out], check=True)
        report = json.loads(out.read_bytes())
    # Each run uses a fresh private cwd. Only substitute the exact known path fields.
    for case in report["cases"]:
        root = str(Path(case["cwd"]).parent)
        case["cwd"] = "$RUN/cwd"
        for exchange in case["exchanges"]:
            exchange["stdin"] = (exchange["stdin"].replace(root + "/", "$RUN/")
                                 .replace(str(bins / "marmish-engine-worker"), "$WORKER"))
    return report


def descriptor_probe(driver, bins, host):
    _, ceiling = resource.getrlimit(resource.RLIMIT_NOFILE)
    if ceiling != resource.RLIM_INFINITY and ceiling < HIGH_NOFILE:
        raise RuntimeError(f"high-NOFILE cell requires hard limit {HIGH_NOFILE}, observed {ceiling}")
    fixture = json.loads(driver.FIXTURE.read_bytes())
    with tempfile.TemporaryDirectory(prefix="whole-bridge-", dir="/tmp") as temp:
        root = Path(temp).resolve()
        cwd = root / "cwd"
        cwd.mkdir(mode=0o700)
        request = fixture["request"].replace("@VERSION@", "1")
        for name, path in [("CWD", cwd), ("STATE", root / "state"),
                           ("WORKER", bins / "marmish-engine-worker")]:
            request = request.replace("@" + name + "@", json.dumps(str(path)))
        child = subprocess.run(
            [bins / "marmish-engine-bridge"], cwd=cwd, env=driver.ENVIRONMENT,
            input=request.encode(), capture_output=True, timeout=driver.TIMEOUT,
            umask=driver.UMASK, preexec_fn=lambda: resource.setrlimit(
                resource.RLIMIT_NOFILE, (HIGH_NOFILE, HIGH_NOFILE)))
        state, _ = driver.inspect_state(root)
    result = {"limit": HIGH_NOFILE, "stdout": child.stdout.decode("latin1"),
              "stderr": child.stderr.decode("latin1"), "status": child.returncode,
              "state": state}
    if host == "aarch64-apple-darwin":
        expected = '{"error":{"code":"worker_unavailable"},"id":"1234567890abcdef1234567890abcdef","ok":false}\n'
        check(result["stdout"], expected, "high limit stdout")
        check(result["stderr"], "", "high limit stderr")
        check(result["status"], 0, "high limit exit")
        driver.compare_state("high-limit", fixture["states"]["absent"], state)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ["upstream", "oddlang", "target-dir", "receipt"]:
        parser.add_argument("--" + arg, required=True, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    verify_source(args.upstream)
    if os.geteuid() == 0:
        parser.error("the pinned oracle requires an unprivileged user")
    driver = load_driver(args.oddlang)
    compiler = output(["rustc", "+1.97.1", "-vV"])
    host = next(line.split()[1] for line in compiler.splitlines() if line.startswith("host:"))
    if host not in HOSTS:
        parser.error("unsupported native qualification host " + host)
    bins = args.target_dir / host / "release"
    receipt = {"format": 1, "commit": PIN, "tree": TREE, "cargo_lock_sha256": CARGO_LOCK,
               "rustc": compiler, "cargo": output(["cargo", "+1.97.1", "-V"]),
               "host": host, "system_release": platform.release(),
               "linker": output(["cc", "--version"]).splitlines()[0],
               "oracle_driver_sha256": DRIVER, "oracle_fixture_sha256": FIXTURE, "builds": {}}
    if host.endswith("darwin"):
        receipt["macos"] = output(["sw_vers", "-productVersion"])
        receipt["sdk"] = output(["xcrun", "--sdk", "macosx", "--show-sdk-version"])
    for profile, selection, files in BUILDS:
        opts = ["+1.97.1", "build", "--locked", "--offline", "--release", "--target", host,
                *selection]
        subprocess.run(["cargo", *opts, "--target-dir", args.target_dir], cwd=args.upstream,
                       check=True)
        receipt["builds"][profile] = {"cargo_args": opts, "sha256": {f: sha(bins / f) for f in files}}
    receipt["bridge"] = replay(driver, bins)
    check(replay(driver, bins), receipt["bridge"], "repeated original baseline")
    receipt["high_nofile"] = descriptor_probe(driver, bins, host)
    check(descriptor_probe(driver, bins, host), receipt["high_nofile"], "repeated high-limit probe")
    if host.endswith("darwin"):
        from darwin_effects import sample
        receipt["macos_effects"] = sample(driver, bins)
    receipt["repetitions"] = 2
    verify_source(args.upstream)
    args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
