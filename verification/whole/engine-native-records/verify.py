"""Qualify engine scalar and native path records in the sealed workspace."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

from compose import BASE, GROUPS, WORKSPACE, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
SOURCE = {"engine": "crates/marmish-engine/src/model.rs",
          "native": "crates/marmish-native/src/transport.rs"}
CHANGED = {f"marmish-{name}" for name in GROUPS}

previous = sys.modules.pop("compose")
directory = HERE.parent / "converged-public-api"
sys.path.insert(0, str(directory))
try:
    CONVERGED = runpy.run_path(str(directory / "verify.py"))
finally:
    sys.path.pop(0)
    sys.modules["compose"] = previous
SIX = CONVERGED["SIX"]
run, data, sha, artifact = (SIX[key] for key in ("run", "data", "sha", "artifact"))
ENGINE = CONVERGED["ENGINE"]
LIMIT = CONVERGED["LIMIT"]

TYPES = {
    "engine": {
        "marmish_engine::InputReceipt": ("Copy", "Default", "Eq", "Ord", "Hash", "Serialize", "Deserialize"),
        "marmish_engine::MaintenanceReport": ("Copy", "Eq", "Ord", "Hash", "Serialize", "Deserialize"),
    },
    "native": {"marmish_native::transport::ArtiDirs": ("Copy", "Default", "Eq", "Ord", "Hash", "Serialize", "Deserialize")},
}
BOUNDS = {"Hash": "std::hash::Hash", "Serialize": "serde::Serialize",
          "Deserialize": "for<'de> serde::Deserialize<'de>"}


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
    work.mkdir()
    crate = f"marmish_{label}"
    args = ["rustc", "+1.97.1", "--edition=2021", "-L", f"dependency={deps}",
            "-L", f"dependency={serde.parent}", "--extern", f"serde={serde}",
            "--extern", f"{crate}={library}"]
    binary = work / "probe"
    run(*args, HERE / f"{label}.rs", "-o", binary)
    stdout = run(binary).decode()
    checks = {
        f"{ty}.{name}": ("E0277", f"fn require<T: {BOUNDS.get(name, name)}>() {{}}\n"
                                   f"fn main() {{ require::<{ty}>(); }}")
        for ty, names in TYPES[label].items() for name in names
    }
    if label == "native":
        checks["rootExposure"] = ("E0432", "use marmish_native::ArtiDirs; fn main() {}")
    else:
        checks["privateModule"] = ("E0603", "use marmish_engine::model::InputReceipt; fn main() {}")
        checks["creditWidth"] = ("E0308", "fn main() { let v = 0u64; let _ = marmish_engine::InputReceipt "
                                         "{accepted: 0, available_credit: v, duplicate: false}; }")
    verified = {}
    for name, (expected, expression) in checks.items():
        path = work / "negative.rs"
        path.write_text(expression + "\n")
        proc = subprocess.run([*args, "--error-format=json", path, "-o", binary],
                              text=True, capture_output=True)
        actual = {row["code"]["code"] for line in proc.stderr.splitlines()
                  if (row := json.loads(line)).get("code") and row.get("level") == "error"}
        if proc.returncode == 0 or actual != {expected}:
            raise ValueError(f"unexpected negative check for {name}: {proc.stderr}")
        verified[name] = expected
    return stdout, verified


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
    for label, member in GROUPS.items():
        original = artifact(native, f"marmish_{label}")
        generated = artifact(build, f"marmish_{label}")
        recovered = scratch / f"{label}-record-recovered"
        provenance = recover(label, work / "oddlang", args.original, recovered)
        for name in ("graph.json", "rust.json"):
            if (recovered / name).read_bytes() != (work / "inputs" / member["domain"] / name).read_bytes():
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
    compose(PORTABLE, check=True)
    host, = (line[6:] for line in run("rustc", "+1.97.1", "-vV").decode().splitlines() if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError(f"unsupported native host: {host}")
    with tempfile.TemporaryDirectory(prefix="odd-record-build-") as w, tempfile.TemporaryDirectory(
        prefix="odd-record-oracle-"
    ) as s:
        work, scratch = Path(w).resolve(), Path(s).resolve()

        def witness(product, build):
            return CONVERGED["observe"](args, host, work, scratch, product, build) | observe_records(
                args, product, build, work, scratch,
            )

        receipt = SIX["qualify"](
            args, host, work, scratch, manifest=MANIFEST, base=BASE,
            check=check_product, extra_artifact_oracle=witness,
            extra_denied={
                "original-prototype": args.original / LIMIT["SOURCE"], "constant-caller": LIMIT["FILE"],
                "reader-caller": LIMIT["BUFFERED"]["PRIVATE"] / "observations.rs",
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
