"""Recover the original storage extent, build a sealed vault, and run its upstream value test."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import PIN, ROOT, project, staged

FRAMEWORK = "595fb7a55dc2016cfab70c5b131bf2f6acfe9983"
PORTABLE = ROOT / "oddlang/portable"
PACKAGE = PORTABLE / "vault-fixed-value-package.json"
MODULE = "5389218be38e60288371fe63a79ae71d2e9dbb8408a34bc53e8010f03117359c"
NAME = "tests::strict_public_values"
SCOPE = "local::marmish_vault::Secret::{type}"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(*cmd, **kwargs):
    return subprocess.check_output(cmd, **kwargs)


def check_pin(root, revision):
    if run("git", "rev-parse", "HEAD", cwd=root).decode().strip() != revision or run(
        "git", "status", "--porcelain", cwd=root
    ):
        raise ValueError(f"expected clean pinned checkout at {root}")


def observation(framework, manifest, target):
    return run("bash", framework / "verification/rust-recovery/run.sh", manifest,
               "marmish-vault", "lib", "marmish_vault", target, "default",
               env=dict(os.environ, ODD_RECOVERY_DATA_SHAPES="1"))


def storage(raw, source):
    graph = json.loads(raw)["graph"]
    decl, = [d for d in graph["declarations"] if d["id"] == source]
    member, = decl["fields"]
    return member["type"]


def passed(raw):
    lines = raw.decode().splitlines()
    if lines.count(f"test {NAME} ... ok") != 1 or not any(
        s.startswith("test result: ok. 1 passed; 0 failed; 0 ignored;") for s in lines
    ):
        raise ValueError("expected exactly the original one-test pass")


def link_test(harness, candidate, output, *, check=True):
    deps = candidate / "target/release/deps"
    rlib, = deps.glob("libmarmish_vault-*.rlib")
    return subprocess.run(["rustc", "+1.97.1", "--test", "--edition=2021", "--error-format=json",
                           harness, "-L", f"dependency={deps}", "--extern", f"marmish_vault={rlib}", "-o", output],
                          capture_output=True, check=check)


def qualify(framework, original, cache):
    check_pin(framework, FRAMEWORK)
    check_pin(original, PIN)
    host = next(s[6:] for s in run("rustc", "+1.97.1", "-vV").decode().splitlines() if s.startswith("host: "))
    if host not in {"aarch64-apple-darwin", "aarch64-unknown-linux-gnu"}:
        raise ValueError("unqualified build isolation platform")
    crate = original / "crates/marmish-vault"
    source = (crate / "src/lib.rs").read_bytes()
    marker = b"#[cfg(test)]\nmod tests {"
    before, after = source.split(marker)
    module = marker + after
    if not before or sha(module) != MODULE:
        raise ValueError("unexpected upstream test module")
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory).resolve()
        cli = cache / "compiler/debug/oddlang"
        run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--target-dir", cache / "compiler",
            "--locked", "--offline", cwd=framework)
        raw = observation(framework, crate / "Cargo.toml", host)
        (work / "original.json").write_bytes(raw)
        run(cli, "recover", "fixed-bytes", work / "original.json", HERE / "selection.json", work / "recovered")
        graph = (work / "recovered/graph.json").read_bytes()
        if graph != (PORTABLE / "vault-fixed-bytes-v1/graph.json").read_bytes():
            raise ValueError("recovered contract differs")
        package = staged(work, PACKAGE)
        candidate = work / "generated"
        if not project(work, cli, package, candidate, original, caller=HERE / "selection.json"):
            raise ValueError("missing build isolation")
        expected_type = storage(raw, SCOPE)
        projected = observation(framework, candidate / "Cargo.toml", host)
        declarations = json.loads(projected)["graph"]["declarations"]
        decl, = [d for d in declarations if d["name"] == "Secret"]
        if storage(projected, decl["id"]) != expected_type:
            raise ValueError("owned byte array provider, element or extent differs")
        original_result = run("cargo", "+1.97.1", "test", "--locked", "--offline", "--release",
                              "--target", host, "--target-dir", cache / "reference", "--lib",
                              "--package", "marmish-vault", "--", "--exact", NAME, cwd=original)
        passed(original_result)
        # Build the original test module as a downstream crate. Its super::* resolves through
        # this explicit public import; all test attributes, source bytes and assertions survive.
        harness = work / "unchanged.rs"
        harness.write_bytes(b"pub use marmish_vault::*;\n" + module)
        link_test(harness, candidate, work / "unchanged")
        passed(run(work / "unchanged", "--exact", NAME))

        # Change the recovered extent in the verifier copy and update its pin. The
        # original assertions must reject this otherwise buildable candidate.
        manifest = json.loads(package.read_bytes())
        component, = [part for part in manifest["components"] if "Secret" in part["reexports"]]
        local_graph = package.parent / component["graph"]["path"]
        mutant_graph = json.loads(local_graph.read_bytes())
        definition, = mutant_graph["types"].values()
        if definition["length"] != 32:
            raise ValueError("unexpected baseline extent")
        definition["length"] = 31
        local_graph.write_text(json.dumps(mutant_graph, indent=2))
        component["graph"]["sha256"] = sha(local_graph.read_bytes())
        package.write_text(json.dumps(manifest, indent=2))
        mutant = work / "mutant"
        if not project(work, cli, package, mutant, original, caller=HERE / "selection.json"):
            raise ValueError("missing mutation build isolation")
        # The reference assertion also checks the returned array type. Rust rejects the
        # changed getter before the harness can run.
        rejected = link_test(harness, mutant, work / "mutant-test", check=False)
        diagnostics = [json.loads(line) for line in rejected.stderr.splitlines() if line.startswith(b"{")]
        typed, = [d for d in diagnostics if d.get("level") == "error" and d.get("code")]
        if (rejected.returncode == 0 or typed["code"]["code"] != "E0277" or "[u8; 31]" not in typed["message"]
            or "32]" not in typed["message"] or "Secret::from_slice(&[7; 32])" not in typed["rendered"]):
            raise ValueError("the original assertion failed to reject the changed array type")
        recovery = json.loads((work / "recovered/recovery.lock").read_bytes())
        return dict(framework=FRAMEWORK, original=PIN, host=host, profile="release",
                    test=NAME, unchangedModuleSha256=MODULE, sourceSha256=sha(source),
                    graphSha256=sha(graph), selectedStorage=expected_type,
                    packageManifestSha256=sha(PACKAGE.read_bytes()), packageLockSha256=sha((candidate/"package.lock").read_bytes()),
                    cargoLockSha256=sha((candidate/"Cargo.lock").read_bytes()),
                    sourceReadDenied=True, originalPassed=1, generatedPassed=1,
                    recovery=recovery, widthControl=dict(length=31, phase="compile unchanged test module",
                        diagnostic=typed["code"]["code"], compilerExit=rejected.returncode))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "cache", "receipt"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    data = qualify(args.framework.resolve(), args.original.resolve(), args.cache.resolve())
    args.receipt.write_text(json.dumps(data, indent=2) + "\n")
