"""Qualify the compact package against the checked endpoint source API."""

import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ASSEMBLY = runpy.run_path(str(HERE.parent / "gap-range-assembly/verify.py"))
ENDPOINT = runpy.run_path(str(HERE.parent / "gap-range-endpoint/verify.py"))
ROOT, PORTABLE, BASELINE, MODEL = (ASSEMBLY[key] for key in ("ROOT", "PORTABLE", "BASELINE", "MODEL"))
run, sha, dump, read, pinned = (ASSEMBLY[key] for key in ("run", "sha", "dump", "read", "pinned"))
FRAMEWORK = "f74fea91c697e1e59ba894fb5284cb1717efec0f"
ROLE = "73e9dd4349d990e8dfd7160473799f6ec5b027a7"
COMPACT = "a18c5f83bd1fcf3869fbcde13ff43bd6bed56d76"
MANIFEST = PORTABLE / "engine-gap-range-derived-source-api-package.json"


def verify(work, args):
    if run("git", "diff", "--name-only", "HEAD", cwd=ROOT):
        raise ValueError("expected clean tracked companion inputs")
    companion = run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip()
    native = ENDPOINT["PRIOR"]["PRIOR"]
    native["clean"](args.original, native["PIN"])
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host != "aarch64-apple-darwin" or f"commit-hash: {ENDPOINT['PRIOR']['RUSTC']}" not in version:
        raise ValueError("expected pinned Darwin ARM64 Rust")

    cases = ENDPOINT["validate_contract"]()
    role_bundle = ENDPOINT["BUNDLE"]
    authority = (role_bundle / "wire.json", role_bundle / "hints.json", ENDPOINT["CASES"])
    before = {path: pinned(path, ROLE) for path in authority}
    witness_path = HERE.parent / "gap-range-endpoint" / f"binding-{host}.json"
    witness = json.loads(pinned(witness_path, ROLE))
    for path, field in zip(authority, ("wireContractSha256", "hintsSha256", "casesSha256"), strict=True):
        if sha(path) != witness[field]:
            raise ValueError(f"endpoint qualification did not cover {path.name}")
    raw_lock = HERE.parent / "gap-range-assembly/package.lock"
    earlier = json.loads(pinned(raw_lock, COMPACT))

    inputs = work / "inputs"
    ASSEMBLY["stage"](inputs).unlink()
    spec = read(MANIFEST)
    role_pin = spec["components"][1].pop("sourceApi")
    if spec != json.loads(pinned(ASSEMBLY["MANIFEST"], COMPACT)):
        raise ValueError("compact package changed inputs beyond the role pin")
    role_source = PORTABLE / role_pin["path"]
    if role_source != role_bundle / "rust.json" or sha(role_source) != role_pin["sha256"]:
        raise ValueError("expected the current pinned Rust source API binding")
    overlay_bytes = pinned(role_source, ROLE)
    if sha(role_source) != witness["bindingSha256"]:
        raise ValueError("endpoint qualification used a different Rust binding")
    overlay = inputs / role_pin["path"]
    overlay.parent.mkdir()
    overlay.write_bytes(overlay_bytes)
    manifest = Path(shutil.copy2(MANIFEST, inputs))
    product = work / "product"

    tool = Path(shutil.copy2(args.compiler, work / "oddlang"))
    traits = HERE.parent / "gap-range-record/traits.rs"
    if sha(traits) != witness["sourceApiProbeSha256"] or sha(BASELINE / "probe.rs") != witness["wireProbeSha256"]:
        raise ValueError("checked endpoint used different callers")
    forbidden = (args.original / MODEL, (args.original / MODEL).with_name("lib.rs"),
                 BASELINE / "probe.rs", traits)
    prefix, env = native["JOURNAL"]["sandbox"](
        native["ASSEMBLY"], work, (ROOT, args.original, args.framework, args.compiler.parent), None
    )
    for path in forbidden:
        denied = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if (not path.is_file() or denied.returncode != 1 or denied.stdout
            or not any(marker in denied.stderr for marker in (b"No such file", b"Operation not permitted"))):
            raise ValueError(f"source isolation failed for {path.name}")

    command = [*prefix, tool, "build", "native-package", "rust", manifest]
    run(*command, product, cwd=work, env=env)
    lock_path = product / "package.lock"
    lock = read(lock_path)
    record = lock["components"][1]
    if lock["manifestSha256"] != sha(manifest) or any(
        row.pop("sourceApiSha256") != role_pin["sha256"] for row in (record, record["projection"])
    ):
        raise ValueError("package lock does not pin the checked role binding")
    lock["manifestSha256"] = earlier["manifestSha256"]
    if lock != earlier:
        raise ValueError("compact package lock changed beyond role provenance")
    files = {p.relative_to(product).as_posix(): sha(p) for p in product.rglob("*")
             if p.is_file() and p != lock_path}
    expected = witness["profiles"]["baseline"]["files"].copy()
    del expected["package.lock"]
    if files != expected:
        raise ValueError("generated native package differs from the checked source API build")

    altered = json.loads(overlay_bytes)
    altered["flattenModule"] = not altered["flattenModule"]
    dump(overlay, altered)

    def reject(label, messages):
        output = work / label
        result = subprocess.run([*command, output], cwd=work, env=env, capture_output=True)
        diagnostic = result.stderr.decode().strip()
        if result.returncode == 0 or output.exists() or not all(message in diagnostic for message in messages):
            raise ValueError(f"unexpected result for {label}: {diagnostic}")
        return diagnostic

    diagnostics = {"stale-pin": reject("stale-pin", ("input digest mismatch", role_pin["path"]))}
    changed = read(manifest)
    changed["components"][1]["sourceApi"]["sha256"] = sha(overlay)
    dump(manifest, changed)
    diagnostics["changed-binding"] = reject("changed-binding", ("source API overlay differs from recovered binding",))
    altered = json.loads(overlay_bytes)
    altered["sourceApi"]["fields"][ENDPOINT["STABLE_ID"]]["visibility"] = "private"
    dump(overlay, altered)
    changed["components"][1]["sourceApi"]["sha256"] = sha(overlay)
    dump(manifest, changed)
    diagnostics["private-field"] = reject("private-field", ("unknown variant", "private", "public"))

    caller = work / "caller"
    (caller / "src").mkdir(parents=True)
    for source, file in (("caller.toml", "Cargo.toml"), ("caller.lock", "Cargo.lock"), ("probe.rs", "src/main.rs")):
        shutil.copyfile(BASELINE / source, caller / file)
    run(*prefix, "cargo", "rustc", "--locked", "--offline", "--release", "--", "--cfg", "with_range",
        cwd=caller, env=env)
    executable = caller / "target/release/gap-observer"
    vectors = json.dumps([{"id": row["id"], "wire": row["wire"], "type": "range"} for row in cases]).encode()
    rows = json.loads(run(*prefix, executable, cwd=caller, env=env, input=vectors))
    if rows != witness["observations"] or len(rows) != len(cases):
        raise ValueError("unchanged Rust wire probe differs from the checked endpoint package")
    shutil.copyfile(traits, caller / "src/main.rs")
    run(*prefix, "cargo", "build", "--locked", "--offline", "--release", cwd=caller, env=env)
    if run(*prefix, executable, cwd=caller, env=env) != native["TRAIT_OUTPUT"]:
        raise ValueError("unchanged typed field caller failed")
    if any(path.read_bytes() != data for path, data in before.items()):
        raise ValueError("portable authority or advisory hints changed")
    return {
        "format": "marmish-gap-range-compact-source-api-v1", "framework": FRAMEWORK,
        "companion": companion, "host": host, "rustc": version,
        "frameworkBuildToolchain": args.framework_toolchain, "compilerSha256": sha(tool),
        "verifierSha256": sha(HERE / "verify.py"), "referenceReceiptSha256": sha(witness_path),
        "previousCompactLockSha256": sha(raw_lock), "manifestSha256": sha(MANIFEST),
        "packageLockSha256": sha(lock_path), "sourceApiSha256": sha(role_source),
        "portableInputs": {path.relative_to(ROOT).as_posix(): sha(path) for path in authority},
        "generatedFiles": files, "observations": rows, "rejections": diagnostics,
        "deniedReads": [path.name for path in forbidden],
        "sourceApiProbeSha256": sha(traits), "wireProbeSha256": sha(BASELINE / "probe.rs"),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler_target", "original", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    prior = ENDPOINT["PRIOR"]
    prior["clean"](args.framework, FRAMEWORK)
    args.framework_toolchain = run("rustc", "+1.96.1", "-vV").decode().splitlines()
    if f"commit-hash: {prior['BUILD_RUSTC']}" not in args.framework_toolchain:
        raise ValueError("expected the pinned Oddlang build toolchain")
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"):
        env.pop(key, None)
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=env)
    args.compiler = args.compiler_target / "debug/oddlang"
    with tempfile.TemporaryDirectory(prefix="odd-gap-range-compact-role-") as temp:
        dump(args.receipt, verify(Path(temp).resolve(), args))
