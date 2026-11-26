"""Reorder an actual source record and compare the native and generated Debug output."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace

from verify import HERE, PORTABLE, SIX, SOURCE, artifact, data, observe, recover, run, sha

OLD = b"    pub records_removed: u32,\n    pub spool_bytes_removed: u64,"
NEW = b"    pub spool_bytes_removed: u64,\n    pub records_removed: u32,"


def mutation(args, work):
    root = work / "original"
    run("git", "clone", "--quiet", "--no-checkout", "--local", args.original, root)
    run("git", "-C", root, "checkout", "--quiet", "--detach", SIX["ORIGINAL"])
    source = root / SOURCE["engine"]
    bytes_before = source.read_bytes()
    if bytes_before.count(OLD) != 1:
        raise ValueError("cannot identify the ordered source record")
    source.write_bytes(bytes_before.replace(OLD, NEW))
    selection = work / "selection.json"
    selection.write_text(json.dumps(data(HERE / "engine-selection.json") | {"sourceSha256": sha(source)}))
    recovered = work / "recovered"
    observation = recover("engine", args.compiler, root, recovered, selection)

    reference = SIX["cargo_reference"](
        SimpleNamespace(original=root, original_target=args.mutant_target), "marmish-engine",
    )
    lib, serde = artifact(reference, "marmish_engine"), artifact(reference, "serde")
    expected = observe("engine", lib, lib.parent / "deps", serde, work / "original-caller")

    inputs = work / "inputs"
    inputs.mkdir()
    package = data(PORTABLE / "engine-observational-records-package.json")
    component = package["components"][-1]
    descriptors = [package["cargoLock"], *(
        prior[key] for prior in package["components"][:-1] for key in ("graph", "overlay")
    )]
    for key in descriptors:
        dest = inputs / key["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(PORTABLE / key["path"], dest)
        assert sha(dest) == key["sha256"]
    for role, basename in (("graph", "graph.json"), ("overlay", "rust.json")):
        shutil.copy2(recovered / basename, inputs)
        component[role] = {"path": basename, "sha256": sha(recovered / basename)}
    manifest = inputs / "package.json"
    manifest.write_text(json.dumps(package))
    product = work / "product"
    run(args.compiler, "build", "native-package", "rust", manifest, product)
    generated = SIX["cargo_reference"](
        SimpleNamespace(original=product, original_target=work / "product-target"), "marmish-engine",
    )
    actual_lib = artifact(generated, "marmish_engine")
    actual = observe("engine", actual_lib, actual_lib.parent / "deps", serde, work / "generated-caller")
    if expected != actual or "MaintenanceReport {\n    spool_bytes_removed: 0,\n    records_removed: 0," not in actual[0]:
        raise ValueError("changed record order not observed identically")
    return {"sourceSelection": observation, "output": actual[0], "negativeRustDiagnostics": actual[1],
            "generatedPackageLockSha256": sha(product / "package.lock")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "original", "mutant_target", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=args.original) or run(
        "git", "rev-parse", "HEAD", cwd=args.original
    ).decode().strip() != SIX["ORIGINAL"]:
        raise ValueError("expected a clean pinned original checkout")
    with tempfile.TemporaryDirectory(prefix="odd-record-mutation-") as directory:
        receipt = mutation(args, Path(directory).resolve())
    receipt.update(framework=args.framework_revision, original=SIX["ORIGINAL"],
                   rustc=run("rustc", "+1.97.1", "-vV").decode().splitlines(),
                   compilerSha256=sha(args.compiler))
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
