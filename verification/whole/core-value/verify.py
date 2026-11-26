#!/usr/bin/env python3
"""Compare one Rust caller against the original and graph-generated value API."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
UPSTREAM_TEST = "retention_distinguishes_default_from_disabled_and_bounds_numbers"
RUST = ("rustc", "+1.97.1")
CARGO = ("cargo", "+1.97.1")
HERE = Path(__file__).resolve().parent
CONTRACT = HERE.parents[2] / "oddlang/portable/core-config-v1"


def run(*args, cwd=None, status=0):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if result.returncode != status:
        raise RuntimeError(
            f"expected exit {status}, got {result.returncode}: {args}\n"
            f"{result.stdout}{result.stderr}"
        )
    return result.stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    return sha(path.read_bytes())


def generate(oddlang, work, domain, overlay, name, supplied=None):
    source = work / f"{name}-graph"
    source.mkdir()
    (source / "domain.ttl").write_bytes(domain)
    binding = source / "rust.json"
    binding.write_bytes(overlay)
    output = work / f"{name}-release"
    run(oddlang, "build", "value-domain", "rust", source, binding, output, cwd=work)
    pins = json.loads((output / "projection.lock").read_text())
    expected = {
        "format": "oddlang-value-domain-v1",
        "domainSha256": pins.get("domainSha256"),
        "bindingSha256": pins.get("bindingSha256"),
        "artifact": "value.rs",
        "artifactSha256": file_sha(output / "value.rs"),
        "obligationsSha256": file_sha(output / "obligations.json"),
    }
    if pins != expected or any(
        not re.fullmatch(r"[0-9a-f]{64}", pins.get(key) or "")
        for key in ("domainSha256", "bindingSha256")
    ):
        raise ValueError("projection lock differs from actual graph and generated artifacts")
    if supplied:
        for file in ("value.rs", "obligations.json", "projection.lock"):
            if (output / file).read_bytes() != (supplied / file).read_bytes():
                raise ValueError(f"fresh graph-only generation differs from supplied {file}")
    library = output / "libmarmish_core.rlib"
    run(*RUST, "--edition=2021", "--crate-type=rlib", "--crate-name=marmish_core",
        output / "value.rs", "-o", library, cwd=work)
    return library, pins


def check_caller(work, source, name, library, dependencies=None):
    binary = work / name
    args = ["-L", f"dependency={dependencies}"] if dependencies else []
    run(*RUST, "--edition=2021", "--test", source, "--extern", f"marmish_core={library}",
        *args, "-o", binary, cwd=work)
    return binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("generated", type=Path)
    parser.add_argument("oddlang", type=Path)
    parser.add_argument("receipt", type=Path)
    options = parser.parse_args()
    original = options.original.resolve()
    compiler = run(*RUST, "-vV")
    if f"commit-hash: {COMPILER}" not in compiler.splitlines():
        raise ValueError("unexpected Rust compiler")
    if run("git", "rev-parse", "HEAD", cwd=original).strip() != ORIGINAL:
        raise ValueError("unexpected original source revision")
    if run("git", "status", "--porcelain", cwd=original):
        raise ValueError("original source checkout has local modifications")

    original_test = original / "crates/marmish-core/tests/contracts.rs"
    unchanged = re.search(
        rb"(?m)^#\[test\]\nfn " + UPSTREAM_TEST.encode() + rb"\(\) \{\n.*?^\}\n",
        original_test.read_bytes(),
        re.DOTALL,
    )
    if not unchanged:
        raise ValueError("original retention test function missing")
    domain = (CONTRACT / "domain.ttl").read_bytes()
    overlay = (CONTRACT / "rust.json").read_bytes()
    old_limit = b"c:maximum 31536000"
    if domain.count(old_limit) != 1:
        raise ValueError("upper bound mutation must alter exactly one triple")
    mutant = domain.replace(old_limit, b"c:maximum 31535999", 1)
    with tempfile.TemporaryDirectory(prefix="oddlang-core-value-") as temporary:
        work = Path(temporary)
        # Only the graph and Rust binding enter either projection.
        generated, pins = generate(
            options.oddlang.resolve(), work, domain, overlay, "generated", options.generated.resolve()
        )
        if json.loads((CONTRACT / "rust.lock").read_bytes()) != pins:
            raise ValueError("committed Rust projection lock mismatch")
        cli = options.oddlang.resolve()
        node_release = work / "typescript"
        run(cli, "build", "value-domain", "typescript", CONTRACT, node_release)
        if (node_release / "projection.lock").read_bytes() != (CONTRACT / "typescript.lock").read_bytes():
            raise ValueError("committed TypeScript projection lock mismatch")
        node_result = json.loads(run("node", "--experimental-strip-types", HERE.parent / "core-node/check.mjs", node_release))
        observed = work / "observation"
        run(cli, "recover", "value-domain", original / "Cargo.toml", CONTRACT / "selection.json", observed)
        for file in ("domain.ttl", "observation.json"):
            if (observed / file).read_bytes() != (CONTRACT / file).read_bytes():
                raise ValueError(f"source recovery differs from committed {file}")
        changed, changed_pins = generate(options.oddlang.resolve(), work, mutant, overlay, "mutant")
        if (changed_pins["domainSha256"] == pins["domainSha256"]
                or changed_pins["bindingSha256"] != pins["bindingSha256"]):
            raise ValueError("mutation must alter the projected domain but not its Rust binding")

        cargo = (*CARGO, "build", "--locked", "--offline", "--release", "-p", "marmish-core",
                 "--target-dir", work / "upstream", "--message-format=json")
        artifacts = [json.loads(line) for line in run(*cargo, cwd=original).splitlines()]
        libraries = [
            Path(filename)
            for item in artifacts
            if item["reason"] == "compiler-artifact" and item["target"]["name"] == "marmish_core"
            for filename in item["filenames"]
            if filename.endswith(".rlib")
        ]
        if len(libraries) != 1:
            raise ValueError("expected exactly one original marmish_core rlib")
        library = libraries[0]

        upstream = run(
            *CARGO, "test", "--locked", "--offline", "--release", "-p", "marmish-core",
            "--target-dir", work / "upstream", "--test", "contracts", "--",
            UPSTREAM_TEST, "--exact", cwd=original,
        )
        if f"test {UPSTREAM_TEST} ... ok" not in upstream:
            raise ValueError("the unchanged original integration test did not execute")

        caller = HERE / "caller.rs"
        harness = work / "caller.rs"
        harness.write_bytes(caller.read_bytes() + b"\n" + unchanged.group())
        original_runner = check_caller(work, harness, "original", library, library.parent / "deps")
        generated_runner = check_caller(work, harness, "generated", generated)
        mutant_runner = check_caller(work, harness, "mutant", changed)

        transcripts = {}
        for name, binary in (("original", original_runner), ("generated", generated_runner)):
            output = run(binary, "--show-output", "--test-threads=1", cwd=work)
            if "5 passed; 0 failed" not in output:
                raise ValueError(f"{name} did not execute the full caller")
            observations = [line for line in output.splitlines() if line.startswith("OBS\t")]
            if len(observations) != 790:
                raise ValueError(f"{name} emitted {len(observations)} observations")
            transcripts[name] = observations
        if transcripts["original"] != transcripts["generated"]:
            raise ValueError("original and generated observations differ")

        failure = run(mutant_runner, "independent_boundaries", "--exact", "--test-threads=1",
                      cwd=work, status=101)
        expected_failure = "rejected allowed input Some(31536000)"
        if expected_failure not in failure:
            raise ValueError("boundary oracle rejected mutation for an unexpected reason")

        receipt = {
            "original": {
                "revision": ORIGINAL,
                "tree": run("git", "rev-parse", "HEAD^{tree}", cwd=original).strip(),
                "cargoLockSha256": file_sha(original / "Cargo.lock"),
                "contractsFileSha256": file_sha(original_test),
                "retentionTestBodySha256": sha(unchanged.group()),
                "retentionTest": UPSTREAM_TEST,
                "retentionTestResult": "passed",
            },
            "generated": {
                "projectionLock": pins,
                "domainFileSha256": sha(domain),
                "rustBindingFileSha256": sha(overlay),
                "regeneratedFromGraphAndBinding": True,
            },
            "typescript": node_result,
            "rustc": compiler.splitlines(),
            "callerSha256": file_sha(caller),
            "cases": {
                "boundaryInputs": 14,
                "seededInputs": 768,
                "errorMessages": 7,
                "publicSurface": 1,
                "unchangedTestFunctionOnBoth": True,
            },
            "observations": {
                name: {"count": len(rows), "sha256": sha(("\n".join(rows) + "\n").encode())}
                for name, rows in transcripts.items()
            },
            "mutation": {
                "graphSha256": sha(mutant),
                "projectionLock": changed_pins,
                "maximum": 31535999,
                "control": expected_failure,
                "result": "rejected",
            },
        }
        options.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
