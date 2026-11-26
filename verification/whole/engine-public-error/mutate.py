"""Qualify a source mutation in the pinned original and isolated engine projection."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from types import SimpleNamespace

from verify import HERE, PORTABLE, SOURCE, SIX, artifact, data, observe, recovery, run, sha

CHANGES = [
    (b"    PathSearchExhausted,\n    ExecutableMissing,", b"    ExecutableMissing,\n    PathSearchExhausted,"),
    (b'#[error("engine busy")]', b'#[error("engine occupied")]'),
    (b"        next_offset: u64,\n        available_credit: u32,",
     b"        available_credit: u32,\n        next_offset: u64,"),
]


def mutate(root, args, compiler, original):
    checkout = root / "upstream"
    run("git", "clone", "--quiet", "--no-checkout", "--local", original, checkout)
    run("git", "-C", checkout, "checkout", "--quiet", "--detach", SIX["ORIGINAL"])
    source = checkout / SOURCE
    source_bytes = source.read_bytes()
    for old, new in CHANGES:
        if source_bytes.count(old) != 1:
            raise ValueError("ambiguous source mutation")
        source_bytes = source_bytes.replace(old, new)
    source.write_bytes(source_bytes)
    selection = root / "selector.json"
    selection.write_text(json.dumps(data(HERE / "selection.json") | {"sourceSha256": sha(source)}))
    evidence = recovery(compiler, source, root / "recovered", selection=selection)
    original_build = SIX["cargo_reference"](
        SimpleNamespace(original=checkout, original_target=args.mutant_target), "marmish-engine",
    )
    reference = artifact(original_build, "marmish_engine")
    serde = artifact(original_build, "serde")
    reference_out = observe(reference, reference.parent / "deps", serde, root / "reference")

    spec = data(PORTABLE / "engine-public-error-package.json")
    policies = root / "inputs"
    policies.mkdir()
    new = spec["components"][-1]
    for descriptor in (spec["cargoLock"], spec["components"][0]["graph"], spec["components"][0]["overlay"]):
        path = policies / descriptor["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(PORTABLE / descriptor["path"], path)
        assert sha(path) == descriptor["sha256"]
    for key, name in [("graph", "graph.json"), ("overlay", "rust.json")]:
        recovered = root / "recovered" / name
        shutil.copy2(recovered, policies)
        new[key] = {"path": name, "sha256": sha(recovered)}
    package = policies / "package.json"
    package.write_text(json.dumps(spec))
    product = root / "portable"
    run(compiler, "build", "native-package", "rust", package, product)
    generated_build = SIX["cargo_reference"](
        SimpleNamespace(original=product, original_target=root / "target"), "marmish-engine",
    )
    generated = artifact(generated_build, "marmish_engine")
    actual = observe(generated, generated.parent / "deps", serde, root / "actual")
    if actual != reference_out:
        raise ValueError("native observations of the changed source differ")
    stdout, negatives = actual
    if ("reason|PathSearchExhausted|1" not in stdout or "reason|ExecutableMissing|0" not in stdout
            or "error|Busy|engine occupied|" not in stdout
            or "InputBackpressure { available_credit: 0, next_offset: 0 }" not in stdout):
        raise ValueError("source edits did not alter the expected native behaviors")
    return {"original": SIX["ORIGINAL"], "recovery": evidence, "output": stdout,
            "negativeRustDiagnostics": negatives,
            "generatedPackageLockSha256": sha(product / "package.lock")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "original", "mutant_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    if run("git", "-C", args.original, "rev-parse", "HEAD").decode().strip() != SIX["ORIGINAL"] or run(
        "git", "-C", args.original, "status", "--porcelain", "--untracked-files=all"
    ):
        raise ValueError("expected clean pinned original")
    with tempfile.TemporaryDirectory(prefix="odd-engine-error-mutation-") as directory:
        receipt = mutate(Path(directory).resolve(), args, args.compiler, args.original)
    receipt["framework"] = args.framework_revision
    receipt["compilerSha256"] = sha(args.compiler)
    receipt["rustc"] = subprocess.check_output(["rustc", "+1.97.1", "-vV"], text=True).splitlines()
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
