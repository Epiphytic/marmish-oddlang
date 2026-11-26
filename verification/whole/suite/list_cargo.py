"""Enumerate compiled Rust test harnesses of the pinned original, including ignored cases."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from inventory import PIN, git


def command(args, cwd):
    return subprocess.check_output(args, cwd=cwd, text=True)


def cases(executable, cwd, ignored=False):
    args = [executable, "--list", "--format", "terse"]
    if ignored:
        args.append("--ignored")
    found = {}
    for line in command(args, cwd).splitlines():
        if not line:
            continue
        name, sep, kind = line.rpartition(": ")
        if not sep or kind not in {"test", "benchmark"} or name in found:
            raise ValueError(f"unexpected harness output {line!r}")
        found[name] = kind
    return found


def inventory(source, target):
    if git(source, "rev-parse", "HEAD").decode().strip() != PIN or git(source, "status", "--porcelain"):
        raise ValueError("expected a clean upstream reference checkout")
    cargo = ["cargo", "+1.97.1"]
    metadata = json.loads(command(cargo + ["metadata", "--format-version=1", "--locked", "--offline",
                                          "--no-deps"], source))
    packages = {p["id"]: p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]}
    build = command(cargo + ["test", "--workspace", "--release", "--locked", "--offline", "--no-run",
                             "--message-format=json", "--target-dir", target], source)
    harnesses = {}
    for line in build.splitlines():
        event = json.loads(line)
        if event["reason"] != "compiler-artifact" or not event["profile"]["test"] or not event["executable"]:
            continue
        package = packages[event["package_id"]]
        executable = Path(event["executable"])
        cwd = Path(package["manifest_path"]).parent
        entry = event["target"]
        key = "/".join([package["name"], "+".join(entry["kind"]), entry["name"]])
        if key in harnesses:
            raise ValueError(f"ambiguous test harness {key}")
        names = cases(executable, cwd)
        ignored = cases(executable, cwd, ignored=True)
        if not ignored.items() <= names.items():
            raise ValueError(f"ignored case absent from full list in {key}")
        with executable.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        harnesses[key] = {"source": str(Path(entry["src_path"]).relative_to(source)),
                          "binarySha256": digest, "features": sorted(event["features"]),
                          "tests": sorted(n for n,k in names.items() if k == "test" and n not in ignored),
                          "ignored": sorted(ignored), "benchmarks": sorted(n for n,k in names.items() if k == "benchmark")}
    if not harnesses or not any(h["tests"] for h in harnesses.values()):
        raise ValueError("build yielded no workspace tests")
    return {"upstream": PIN, "cargoLockSha256": hashlib.sha256((source / "Cargo.lock").read_bytes()).hexdigest(),
            "rustc": command(["rustc", "+1.97.1", "-vV"], source).strip(),
            "selection": "native release workspace, default features", "harnesses": dict(sorted(harnesses.items()))}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("target_dir", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = inventory(args.upstream.resolve(), args.target_dir.resolve())
    args.output.write_text(json.dumps(report, indent=2) + "\n")
