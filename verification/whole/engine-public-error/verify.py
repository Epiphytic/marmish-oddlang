"""Compare pure engine errors and native identity in a sealed seven-member build."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

from compose import DOMAIN, WORKSPACE, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
SOURCE = Path("crates/marmish-engine/src/model.rs")
CHANGED = {"marmish-engine", "marmish-native"}

# Load the existing seven-member lane with its own compose module.
sys.path.insert(0, str(HERE.parent / "unified-buffered"))
current_compose = sys.modules.pop("compose")
try:
    BUFFERED = runpy.run_path(str(HERE.parent / "unified-buffered/verify.py"))
finally:
    sys.path.pop(0)
    sys.modules["compose"] = current_compose
SIX = BUFFERED["SIX"]
IDENTITY = SIX["lane"]("backend-choice")
run, data, sha, artifact = (SIX[key] for key in ("run", "data", "sha", "artifact"))
BASE = BUFFERED["WORKSPACE"]

TRAITS = {
    "Clone": "Clone",
    "Copy": "Copy",
    "Default": "Default",
    "Hash": "std::hash::Hash",
    "PartialOrd": "PartialOrd",
    "Ord": "Ord",
    "Serialize": "serde::Serialize",
    "Deserialize": "for<'de> serde::Deserialize<'de>",
    "FromIo": "From<std::io::Error>",
}
REASON_TRAITS = {
    name: TRAITS[name]
    for name in ("Default", "Hash", "PartialOrd", "Ord", "Serialize", "Deserialize")
} | {"Display": "std::fmt::Display", "Error": "std::error::Error"}
REASONS = ["NotFoundReason", "NotExecutableReason", "ResolutionReason"]


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (MANIFEST.with_suffix("") / "workspace.lock").read_bytes():
        raise ValueError("typed engine workspace differs from its pin")
    current = data(lock)
    previous = data((PORTABLE / BASE).with_suffix("") / "workspace.lock")
    if current["cargoLockSha256"] != previous["cargoLockSha256"]:
        raise ValueError("typed engine changed the dependency lock")
    members, old = current["members"], previous["members"]
    if (members.keys() != old.keys()
            or {name for name in members if members[name] != old[name]} != CHANGED
            or any(members[name]["dependencies"] != old[name]["dependencies"] for name in old)):
        raise ValueError("unexpected change to predecessor member pins or edges")
    SIX["check_member_files"](product, current)
    return current


def observe(rlib, deps, serde, scratch):
    scratch.mkdir()
    args = ["rustc", "+1.97.1", "--edition=2021", "-L", f"dependency={deps}",
            "-L", f"dependency={serde.parent}", "--extern", f"serde={serde}",
            "--extern", f"marmish_engine={rlib}"]
    binary = scratch / "test"
    run(*args, HERE / "probe.rs", "-o", binary)
    output = run(binary).decode()
    negatives = {
        **{f"EngineError.{name}": ("E0277", f"require::<EngineError>();", bound)
           for name, bound in TRAITS.items()},
        **{f"{name}.{trait}": ("E0277", f"require::<{name}>();", bound)
           for name in REASONS for trait, bound in REASON_TRAITS.items()},
        "mixedReason": ("E0308", "let _ = EngineError::CommandNotFound(NotExecutableReason::NotRegularFile);", "Sized"),
        "offsetWidth": ("E0308", "let x = u128::MAX; let _ = EngineError::InputBackpressure { next_offset: x, available_credit: 0 };", "Sized"),
        "creditWidth": ("E0308", "let x = u64::MAX; let _ = EngineError::InputBackpressure { next_offset: 0, available_credit: x };", "Sized"),
        "privateModule": ("E0603", "use marmish_engine::model::EngineError as Hidden;", "Sized"),
        "nonExhaustiveMatch": ("E0004", "match EngineError::Unauthorized { EngineError::Unauthorized => () }", "Sized"),
    }
    verified = {}
    for name, (expected, expression, bound) in negatives.items():
        path = scratch / "negative.rs"
        path.write_text(f"use marmish_engine::{{EngineError, {', '.join(REASONS)}}};\n"
                        f"fn require<T: {bound}>() {{}}\nfn main() {{ {expression} }}\n")
        call = subprocess.run([*args, "--error-format=json", path, "-o", binary],
                              capture_output=True, text=True)
        codes = {item["code"]["code"] for line in call.stderr.splitlines()
                 if (item := json.loads(line)).get("level") == "error" and item.get("code")}
        if call.returncode == 0 or codes != {expected}:
            raise ValueError(f"unexpected negative diagnostic {name}: {call.stderr}")
        verified[name] = expected
    return output, verified


def recovery(compiler, source, output, *, selection=HERE / "selection.json"):
    run(compiler, "recover", "finite-error-domain", source, selection, output)
    return data(output / "observation.json")


def observe_workspace(args, host, work, scratch, product, build, compiler=None):
    # This runs only after the sandboxed workspace build and the inherited oracles.
    compiler = compiler or work / "oddlang"
    prior = BUFFERED["buffer_oracle"](product, args.original, host)
    recovered = scratch / "engine-recovered"
    observation = recovery(compiler, args.original / SOURCE, recovered)
    for name in ("graph.json", "rust.json"):
        if (recovered / name).read_bytes() != (work / "inputs" / DOMAIN / name).read_bytes():
            raise ValueError(f"recovered engine {name} differs from the staged input")
    engine = SIX["cargo_reference"](args, "marmish-engine")
    reference = artifact(engine, "marmish_engine")
    serde = artifact(engine, "serde")
    sealed = artifact(build, "marmish_engine")
    deps = product / "target/release/deps"
    original = observe(reference, reference.parent / "deps", serde, scratch / "engine-original")
    generated = observe(sealed, deps, serde, scratch / "engine-generated")
    if original != generated:
        raise ValueError("sealed engine type behavior differs")
    output, negatives = generated
    categories = {line.split("|")[1] for line in output.splitlines() if line.startswith("error|")}
    if len(categories) != 26:
        raise ValueError("engine caller failed to exercise every category")
    oracle = {
        "scope": ["EngineError", *REASONS, "Result"],
        "originalLibrarySha256": sha(reference), "sealedLibrarySha256": sha(sealed),
        "recovery": observation, "output": output, "negativeRustDiagnostics": negatives,
        "distinctVariants": len(categories),
    }
    native = SIX["cargo_reference"](args, "marmish-native")
    ref = artifact(native, "marmish_native")
    serde = artifact(native, "serde")
    sealed = artifact(build, "marmish_native")
    orig_choice = IDENTITY["observe"](ref, ref.parent / "deps", serde, serde.parent, scratch / "choice-original")
    gen_choice = IDENTITY["observe"](sealed, deps, serde, serde.parent, scratch / "choice-generated")
    if orig_choice != gen_choice:
        raise ValueError("sealed identity choice differs from original")
    values, negatives = gen_choice
    identity = {"scope": "marmish_native::identity::BackendChoice",
                "output": values.decode(), "negativeRustDiagnostics": negatives}
    return prior | {"enginePublicError": oracle, "identity": identity}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda path: Path(path).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for path, revision in ((args.framework, args.framework_revision), (args.original, SIX["ORIGINAL"]),
                           (ROOT, None)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=path) or (
            revision and run("git", "rev-parse", "HEAD", cwd=path).decode().strip() != revision
        ):
            raise ValueError(f"expected clean checkout {revision or ''}: {path}")
    host, = (line.removeprefix("host: ") for line in run("rustc", "+1.97.1", "-vV").decode().splitlines()
             if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError(f"unsupported native qualification host {host}")
    compose(PORTABLE, check=True)
    with tempfile.TemporaryDirectory(prefix="odd-engine-types-") as work, tempfile.TemporaryDirectory(
        prefix="odd-engine-oracle-"
    ) as scratch:
        work, scratch = Path(work).resolve(), Path(scratch).resolve()
        receipt = SIX["qualify"](
            args, host, work, scratch,
            manifest=MANIFEST, base=BASE, member="marmish-engine", check=check_product,
            extra_denied={
                "original-prototype": args.original / BUFFERED["BUFFER"]["SOURCE"],
                "buffer-caller": BUFFERED["PRIVATE"] / "observations.rs",
                "original-engine-error": args.original / SOURCE,
                "engine-error-caller": HERE / "probe.rs",
                "original-backend-choice": args.original / IDENTITY["SOURCE"],
                "backend-choice-caller": HERE.parent / "backend-choice/probe.rs",
            },
            extra_oracle=lambda product, build: observe_workspace(args, host, work, scratch, product, build),
            receipt_format="marmish-seven-member-typed-error-identity-v1",
        )
        receipt["unchangedMemberLocks"] = sorted(set(receipt["physicalMembers"]) - CHANGED)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
