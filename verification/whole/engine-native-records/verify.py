"""Qualify engine scalar and native path records in the sealed workspace."""
import argparse
import json
from pathlib import Path
import runpy
import tempfile

BASE = "workspace-v7-converged-public-api.json"
WORKSPACE = "workspace-v7-observational-records.json"
DOMAINS = {"engine": "engine-scalar-records-v1", "native": "native-path-records-v1"}

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
SOURCE = {"engine": "crates/marmish-engine/src/model.rs",
          "native": "crates/marmish-native/src/transport.rs"}
CHANGED = {f"marmish-{name}" for name in DOMAINS}

ENGINE = runpy.run_path(str(HERE.parent / "engine-public-error/verify.py"))
SIX = ENGINE["SIX"]
run, data, sha, artifact = (SIX[key] for key in ("run", "data", "sha", "artifact"))

TYPES = {
    "engine": ["marmish_engine::InputReceipt", "marmish_engine::MaintenanceReport"],
    "native": ["marmish_native::transport::ArtiDirs"],
}


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (MANIFEST.with_suffix("") / "workspace.lock").read_bytes():
        raise ValueError("record workspace differs from its pin")
    current = data(lock)
    prior = data((PORTABLE / BASE).with_suffix("") / "workspace.lock")
    if any(current[k] != prior[k] for k in ["cargoLockSha256", "workspaceCargoManifestSha256"]):
        raise ValueError("record workspace dependency or package manifest changed")
    members, old = current["members"], prior["members"]
    if members.keys() != old.keys() or {name for name in old if members[name] != old[name]} != CHANGED:
        raise ValueError("unexpected change to previous member locks")
    if any(members[name]["dependencies"] != old[name]["dependencies"] for name in old):
        raise ValueError("record workspace changed sibling edges")
    SIX["check_member_files"](product, current)
    return current


def observe(label, library, deps, serde, work):
    role = {"engine": "records", "native": "native"}[label]
    return tuple(SIX["product_witness"]("engine-caller", role, library, deps, serde, work))


def recover(label, compiler, original, output, selection=None):
    run(compiler, "recover", "public-root-records", original / SOURCE[label],
        selection or HERE / f"{label}-selection.json", output)
    return data(output / "recovery.lock")


def observe_records(args, product, build, work, scratch):
    native = SIX["cargo_reference"](args, "marmish-native")
    reference_native = artifact(native, "marmish_native")
    reference_deps = reference_native.parent / "deps"
    reference_serde = ENGINE["serde_for_native"](native, reference_native, reference_deps, scratch / "record-ref-serde")
    generated_native = artifact(build, "marmish_native")
    generated_deps = product / "target/release/deps"
    generated_serde = ENGINE["serde_for_native"](build, generated_native, generated_deps, scratch / "record-gen-serde")
    result = {}
    for label, domain in DOMAINS.items():
        original = artifact(native, f"marmish_{label}")
        generated = artifact(build, f"marmish_{label}")
        recovered = scratch / f"{label}-record-recovered"
        provenance = recover(label, work / "oddlang", args.original, recovered)
        for name in ("graph.json", "rust.json"):
            if (recovered / name).read_bytes() != (work / "inputs" / domain / name).read_bytes():
                raise ValueError(f"{label} record recovery differs from staged inputs")
        expected = observe(label, original, reference_deps, reference_serde, scratch / f"{label}-record-original")
        got = observe(label, generated, generated_deps, generated_serde, scratch / f"{label}-record-generated")
        if got != expected:
            raise ValueError(f"sealed {label} records differ from original values")
        output, negatives = got
        result[label] = {"scope": list(TYPES[label]), "output": output, "negativeRustDiagnostics": negatives,
                         "recovery": provenance, "originalLibrarySha256": sha(original),
                         "generatedLibrarySha256": sha(generated)}
    return {"publicRecords": result}


def observe_public_api(args, host, work, scratch, product):
    return SIX["product_witness"]("manifest-oracle", product, args.original, args.original_target,
                                  scratch, host, scratch / "workspace-cargo-artifacts.jsonl",
                                  "--inherited-work", work)


PROTOTYPE_SOURCE = "crates/marmish-prototype/src/lib.rs"
CONSTANT_CALLER = HERE.parent / "prototype-constant/caller.rs"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda path: Path(path).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for checkout, rev in ((args.framework, args.framework_revision), (args.original, SIX["ORIGINAL"]), (ROOT, None)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=checkout) or (
            rev and run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() != rev
        ):
            raise ValueError(f"expected clean checkout {rev or ''}: {checkout}")
    SIX["check_api_release"]("observational-records", PORTABLE)
    host, = (line[6:] for line in run("rustc", "+1.97.1", "-vV").decode().splitlines() if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError(f"unsupported native host: {host}")
    with tempfile.TemporaryDirectory(prefix="odd-record-build-") as w, tempfile.TemporaryDirectory(
        prefix="odd-record-oracle-"
    ) as s:
        work, scratch = Path(w).resolve(), Path(s).resolve()

        def witness(product, build):
            return observe_public_api(args, host, work, scratch, product) | observe_records(
                args, product, build, work, scratch,
            )

        receipt = SIX["qualify"](
            args, host, work, scratch, manifest=MANIFEST, base=BASE,
            check=check_product, extra_artifact_oracle=witness,
            extra_denied={
                "original-prototype": args.original / PROTOTYPE_SOURCE, "constant-caller": CONSTANT_CALLER,
                "reader-caller": HERE.parent / "private-buffer/observations.rs",
                "original-engine-error": args.original / ENGINE["SOURCE"],
                "engine-error-caller": ENGINE["HERE"] / "probe.rs",
                "original-native-choice": args.original / ENGINE["IDENTITY"]["SOURCE"],
                "choice-caller": HERE.parent / "backend-choice/probe.rs",
                "original-public-path-record": args.original / SOURCE["native"],
                "engine-record-caller": HERE / "engine.rs", "native-record-caller": HERE / "native.rs",
            }, receipt_format="marmish-converged-observational-records-v1",
        )
        receipt["unchangedMemberLocks"] = sorted(set(receipt["physicalMembers"]) - CHANGED)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
