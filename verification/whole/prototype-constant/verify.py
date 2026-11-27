"""Qualify the prototype crate-root MAX_LINE export in the sealed workspace."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from compose import BASE, CONTRACT, MEMBER, PREVIOUS_PACKAGE, WORKSPACE, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
SOURCE = "crates/marmish-prototype/src/lib.rs"
FILE = HERE / "caller.rs"
PREVIOUS = PORTABLE / BASE
PRIVATE_LEAF = "src/group_0/__projection_0.rs"


BUFFERED = runpy.run_path(str(HERE.parent / "unified-buffered/workspace_compat.py"))
SIX = BUFFERED["SIX"]
run, data, sha = (SIX[k] for k in ("run", "data", "sha"))


def prior_product(compiler, scratch):
    prior = scratch / "prior"
    run(compiler, "build", "native-package", "rust", PORTABLE / PREVIOUS_PACKAGE, prior)
    return prior


def check_product(product, prior):
    current = data(product / "workspace.lock")
    predecessor = data(PREVIOUS.with_suffix("") / "workspace.lock")
    if (product / "workspace.lock").read_bytes() != (MANIFEST.with_suffix("") / "workspace.lock").read_bytes():
        raise ValueError("successor workspace lock differs")
    if current["cargoLockSha256"] != predecessor["cargoLockSha256"]:
        raise ValueError("Cargo provider pins changed")
    siblings = {name: lock for name, lock in current["members"].items() if name != MEMBER}
    if len(siblings) != 6 or siblings != {k: v for k, v in predecessor["members"].items() if k != MEMBER}:
        raise ValueError("existing six sibling lock rows changed")
    if sha(prior / "package.lock") != predecessor["members"][MEMBER]["standalonePackageLockSha256"]:
        raise ValueError("prior standalone private package lock changed")
    for name, digest in data(prior / "package.lock")["generatedFiles"].items():
        if name != "src/lib.rs" and sha(product / MEMBER / name) != digest:
            raise ValueError("predecessor private module bytes changed")
    SIX["check_member_files"](product, current)
    return current


def recover_from(root, compiler, target, output, tag, env):
    source = root / SOURCE
    run("cargo", "+1.97.1", "rustdoc", "--release", "--locked", "--offline",
        "--target-dir", target, "-p", MEMBER, "--lib", "--", "-Z", "unstable-options",
        "--output-format", "json", cwd=root, env=dict(env, RUSTC_BOOTSTRAP="1"))
    doc = target / "doc/marmish_prototype.json"
    chosen = output / f"{tag}-selection.json"
    chosen.write_text(json.dumps({
        "format": "oddlang-public-constant-selection-v1",
        "id": "urn:marmish:prototype:limits",
        "constantId": "urn:marmish:prototype:max-line",
        "crateName": "marmish_prototype",
        "name": "MAX_LINE",
        "sourceFilename": SOURCE,
        "sourceSha256": sha(source),
        "rustdocSha256": sha(doc),
    }, indent=2) + "\n")
    result = output / tag
    run(compiler, "recover", "public-constant", source, doc, chosen, result)
    return result


def source_oracle(args, compiler, scratch, env):
    baseline = recover_from(args.original, compiler, args.original_target, scratch, "baseline", env)
    for artifact in ("graph.json", "rust.json"):
        if (baseline / artifact).read_bytes() != (PORTABLE / CONTRACT / artifact).read_bytes():
            raise ValueError("recovered baseline differs from released graph or overlay")
    changed = scratch / "changed"
    changed.mkdir()
    archive = subprocess.Popen(
        ["git", "-C", args.original, "archive", SIX["ORIGINAL"]], stdout=subprocess.PIPE
    )
    with archive.stdout:
        subprocess.run(["tar", "-xf", "-", "-C", changed], stdin=archive.stdout, check=True)
    if archive.wait():
        raise ValueError("could not archive the exact original")
    source = changed / SOURCE
    before = b"pub const MAX_LINE: usize = 128 * 1024;"
    original = source.read_bytes()
    if original.count(before) != 1:
        raise ValueError("expected exactly one selected constant expression")
    source.write_bytes(original.replace(before, b"pub const MAX_LINE: usize = 129 * 1024;"))
    mutation = recover_from(changed, compiler, args.original_target, scratch, "mutation", env)
    assert data(mutation / "graph.json")["constants"] == [{
        "id": "urn:marmish:prototype:max-line", "value": {"multiply": [129, 1024]},
    }]
    if (mutation / "rust.json").read_bytes() != (baseline / "rust.json").read_bytes():
        raise ValueError("a value-only change must preserve the native binding")
    events = {tag: data(path / "recovery.lock") for tag, path in
              (("original", baseline), ("mutation", mutation))}
    if [row["unsignedDecimal"] for row in events.values()] != ["131072", "132096"]:
        raise ValueError("incorrect independent native numeric oracle")
    return events


def caller(path, scratch, label):
    out = scratch / label
    deps = path.parent / "deps" if (path.parent / "deps").exists() else path.parent
    run("rustc", "+1.97.1", "--edition=2021", FILE, "-L", f"dependency={deps}",
        "--extern", f"marmish_prototype={path}", "-o", out)
    lines = run(out).decode()
    if lines != "131072\n":
        raise ValueError("MAX_LINE caller emitted unexpected native value")
    return {"librarySha256": sha(path), "stdout": lines}


def reader_oracle(product, args, host, prior):
    return BUFFERED["successor_reader"](product, args.original, host, prior)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda value: Path(value).resolve())
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
    env = SIX["JOURNAL"]["cargo_env"]()
    with tempfile.TemporaryDirectory(prefix="odd-line-input-") as work, tempfile.TemporaryDirectory(
        prefix="odd-line-oracles-"
    ) as tmp:
        scratch = Path(tmp).resolve()
        compiler = Path(work) / "oddlang"
        prior = None

        def check(product):
            nonlocal prior
            prior = prior_product(compiler, scratch)
            return check_product(product, prior)

        def observe(product):
            original = SIX["artifact"](SIX["cargo_reference"](args, MEMBER), "marmish_prototype")
            generated = product / "target/release/libmarmish_prototype.rlib"
            return {"publicConstant": {
                "path": "marmish_prototype::MAX_LINE",
                "callerSha256": sha(FILE),
                "nativeOriginal": caller(original, scratch, "original-caller"),
                "nativeGenerated": caller(generated, scratch, "generated-caller"),
                "sourceRecovery": source_oracle(args, compiler, scratch, env),
            }, "bufferedInput": reader_oracle(product, args, host, prior)}

        receipt = SIX["qualify"](
            args, host, Path(work).resolve(), scratch, manifest=MANIFEST, base=BASE, member=MEMBER,
            check=check, extra_denied={"original-prototype": args.original / SOURCE,
                                       "prototype-constant-caller": FILE,
                                       "buffer-caller": BUFFERED["PRIVATE"] / "observations.rs"},
            extra_oracle=observe, receipt_format="marmish-seven-member-prototype-constant-v1",
        )
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
