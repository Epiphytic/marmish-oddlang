"""Observe the three public diagnostic functions through a downstream Rust caller."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = Path("crates/marmish-native/src/diagnostics.rs")
HOSTS = {"aarch64-apple-darwin": ("macos", "aarch64"), "aarch64-unknown-linux-gnu": ("linux", "aarch64")}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*args, cwd=None):
    result = subprocess.run(args, cwd=cwd, capture_output=True, timeout=1500)
    if result.returncode:
        raise RuntimeError(f"{args[0]} exited {result.returncode}: {result.stderr.decode(errors='replace')}")
    return result.stdout


def expected(host, version):
    platform, arch = HOSTS[host]
    context = dict(schema=1, event="context", platform=platform, architecture=arch, version=version)
    def event(stage, category):
        return dict(schema=1, event="diagnostic", stage=stage, category=category)
    return {
        "disabled": ("before", []),
        "sequence": ("after", [context, event('quoted "\\', 'unicode 雪\n\t\x01'), event("after", "begin"),
                               context, event("", "")]),
        "cross-thread": ("initial", [context, event("joined", "visible")]),
        "closed-sink": ("after-error", []),
    }


def observe(library, host, version, scratch, host_deps=None):
    caller = scratch / "observer"
    native_search = ["-L", f"dependency={host_deps}"] if host_deps else []
    run("rustc", "+1.97.1", "--edition=2021", "--target", host, HERE / "probe.rs",
        "--extern", f"marmish_native={library}", "-L", f"dependency={library.parent / 'deps'}", *native_search, "-o", caller)
    rows = {}
    for case, (state, events) in expected(host, version).items():
        if case == "closed-sink":
            read_fd, write_fd = os.pipe()
            os.close(read_fd)
            with os.fdopen(write_fd, "wb") as broken_pipe:
                proc = subprocess.run([caller, case], stdout=subprocess.PIPE, stderr=broken_pipe, timeout=10)
        else:
            proc = subprocess.run([caller, case], capture_output=True, timeout=10)
        raw = proc.stderr or b""
        if proc.returncode or proc.stdout != (state + "\n").encode():
            raise ValueError(f"{case}: exit {proc.returncode}, public state {proc.stdout!r}")
        expected_bytes = "".join(json.dumps(event, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
                                 + "\n" for event in events).encode()
        if raw != expected_bytes:
            raise ValueError(f"{case}: unexpected event bytes {raw!r}")
        rows[case] = dict(exitCode=proc.returncode, stdout=proc.stdout.decode(), stderr=raw.decode(),
                          sink="closed pipe" if case == "closed-sink" else "captured")
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "cargo_target", "receipt"):
        parser.add_argument(name, type=lambda p: Path(p).resolve())
    args = parser.parse_args()
    if (run("git", "rev-parse", "HEAD", cwd=args.original).decode().strip() != ORIGINAL
            or run("git", "status", "--porcelain", "--untracked-files=all", cwd=args.original)):
        raise ValueError("requires the clean, pinned original checkout")
    compiler = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = (line[6:] for line in compiler if line.startswith("host: "))
    if host not in HOSTS:
        raise ValueError(f"unsupported host: {host}")
    raw = run("cargo", "+1.97.1", "build", "--locked", "--offline", "--release", "--target", host,
              "--target-dir", args.cargo_target, "-p", "marmish-native", "--lib", "--message-format=json",
              cwd=args.original)
    manifest = args.original / "crates/marmish-native/Cargo.toml"
    artifact, = (row for line in raw.splitlines() if (row := json.loads(line))["reason"] == "compiler-artifact"
                 and row["target"]["kind"] == ["lib"] and Path(row["manifest_path"]).resolve() == manifest)
    library, = (Path(path) for path in artifact["filenames"] if path.endswith(".rlib"))
    if library.resolve() != args.cargo_target / host / "release/libmarmish_native.rlib":
        raise ValueError("unexpected compiled artifact")
    with tempfile.TemporaryDirectory() as work:
        rows = observe(library, host, "0.3.1", Path(work), args.cargo_target / "release/deps")
    receipt = dict(original=ORIGINAL, compiler=compiler, host=host, sourceSha256=sha(args.original / SOURCE),
                   packageManifestSha256=sha(manifest), cargoLockSha256=sha(args.original / "Cargo.lock"),
                   probeSha256=sha(HERE / "probe.rs"), librarySha256=sha(library), observations=rows)
    args.receipt.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
