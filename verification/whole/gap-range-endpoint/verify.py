"""Verify the endpoint source-API binding and advisory hint invariance."""

import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
PRIOR = runpy.run_path(str(HERE.parent / "gap-range-recovery/verify.py"))
ROOT, PORTABLE, BASELINE, MODEL = (PRIOR[key] for key in ("ROOT", "PORTABLE", "BASELINE", "MODEL"))
run, sha, dump, read, pinned = (PRIOR[key] for key in ("run", "sha", "dump", "read", "pinned"))
FRAMEWORK = "b530e740a6eaef5e993192777463bf39e222c477"
SEALED = "6355ae8504a7eb64ae01fc0591f29e3879c38128"
STABLE_ID = "urn:marmish:engine:output-gap:range:end"
PROFILE = "engine-gap-range-native-idioms"
BUNDLE = PORTABLE / (PROFILE + "-v1")
MANIFEST = PORTABLE / (PROFILE + "-package.json")
CASES = HERE / "cases.json"


WORKSPACE_TOOLS = runpy.run_path(str(HERE.parent / "unified-mdk/workspace_compat.py"))


def stage(work, profile):
    lane, manifest = WORKSPACE_TOOLS["product_witness"]("endpoint-inputs", work, profile)
    return Path(lane), Path(manifest)


def validate_contract():
    return WORKSPACE_TOOLS["product_witness"]("endpoint-contract")


def verify(work, args):
    native = PRIOR["PRIOR"]
    version = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host not in PRIOR["HOSTS"] or f"commit-hash: {PRIOR['RUSTC']}" not in version:
        raise ValueError("expected the pinned native ARM64 runner")
    tracked_diff = run("git", "diff", "--name-only", "HEAD", "--", cwd=ROOT).decode().splitlines()
    if set(tracked_diff) - {(BUNDLE / "hints.json").relative_to(ROOT).as_posix()}:
        raise ValueError("expected a clean companion checkout apart from advisory hints")
    cases = validate_contract()
    native["clean"](args.original, native["PIN"])
    inputs = (BUNDLE / "wire.json", CASES, BUNDLE / "hints.json", BUNDLE / "rust.json", MANIFEST)
    def input_digests():
        return {path: sha(path) if path.is_file() else None for path in inputs}

    inputs_before = input_digests()
    compiler = Path(shutil.copy2(args.compiler, work / "oddlang"))
    forbidden = (
        args.original / MODEL, (args.original / MODEL).with_name("lib.rs"),
        BASELINE / "probe.rs", HERE.parent / "gap-range-record/traits.rs",
    )
    prefix, env = native["JOURNAL"]["sandbox"](
        native["ASSEMBLY"], work, (ROOT, args.original, args.framework, args.compiler.parent), None
    )
    for path in forbidden:
        deny = subprocess.run([*prefix, "/bin/cat", path], cwd=work, env=env, capture_output=True)
        if (not path.is_file() or deny.returncode != 1 or deny.stdout
            or not any(marker in deny.stderr for marker in (b"No such file", b"Operation not permitted"))):
            raise ValueError(f"source isolation failed for {path.name}")

    def inside(*argv, cwd, **options):
        wrapper = list(prefix)
        if host.endswith("linux-gnu"):
            wrapper[wrapper.index("--chdir") + 1] = str(cwd)
        return run(*wrapper, *argv, cwd=cwd, env=env, **options)

    for profile in ("baseline", "replacement", "without-rust", "without-typescript", "absent"):
        lane, manifest = stage(work, profile)
        product = lane / "product"
        inside(compiler, "build", "native-package", "rust", manifest, product, cwd=lane)
    outputs = WORKSPACE_TOOLS["product_witness"]("endpoint-profiles", work)
    reference_lock = PORTABLE / "engine-gap-range-recovered-v1/package.lock"

    negative, negative_manifest = stage(work, "negative-binding")
    bad_spec = read(negative_manifest)
    overlay_pin = bad_spec["components"][1]["overlay"]
    overlay_path = negative_manifest.parent / overlay_pin["path"]
    bad_overlay = read(overlay_path)
    bad_overlay["sourceApi"]["fields"][STABLE_ID]["visibility"] = "private"
    dump(overlay_path, bad_overlay)
    overlay_pin["sha256"] = sha(overlay_path)
    dump(negative_manifest, bad_spec)
    rejected = negative / "product"
    try:
        inside(compiler, "build", "native-package", "rust", negative_manifest, rejected,
               cwd=negative, stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as error:
        diagnostic = error.output.decode().strip()
        if rejected.exists() or not all(word in diagnostic for word in ("unknown variant", "private", "public")):
            raise ValueError("private binding failed for an unexpected reason") from error
    else:
        raise ValueError("private field was accepted for the public Rust role")

    ts_inputs = {}
    if args.typescript:
        ts = HERE / "typescript"
        js = work / "ts"
        node = run("node", "--version").decode().strip()
        if not node.startswith("v22."):
            raise ValueError("expected Node 22")
        ts_inputs = {name: sha(ts / name) for name in ("endpoint.ts", "check.cjs")}
        run("npx", "--offline", "--package=typescript@5.9.3", "tsc", "--strict",
            "--target", "ES2022", "--module", "commonjs", "--outDir", js, "endpoint.ts", cwd=ts)
        for profile, evidence in outputs.items():
            evidence["typescript"] = run("node", ts / "check.cjs", js / "endpoint.js",
                                         work / profile / CASES.name, cwd=work).decode().strip()

    # All profile products are byte-identical before any caller source is staged.
    caller = work / "baseline/caller"
    (caller / "src").mkdir(parents=True)
    for source, file in (("caller.toml", "Cargo.toml"), ("caller.lock", "Cargo.lock"), ("probe.rs", "src/main.rs")):
        shutil.copyfile(BASELINE / source, caller / file)
    inside("cargo", "rustc", "--locked", "--offline", "--release", "--", "--cfg", "with_range", cwd=caller)
    executable = caller / "target/release/gap-observer"
    observations = read(HERE / f"{host}.json")["original"]["results"]
    for profile in outputs:
        cases_for_profile = read(work / profile / CASES.name)
        vectors = json.dumps([
            {"id": case["id"], "wire": case["wire"], "type": "range"} for case in cases_for_profile
        ]).encode()
        rows = json.loads(inside(executable, cwd=caller, input=vectors))
        if rows != observations or [row["id"] for row in rows] != [case["id"] for case in cases]:
            raise ValueError(f"endpoint behavior changed under {profile}")
        for case, row in zip(cases, rows, strict=True):
            result = row["result"]
            if case["expected"] is None:
                if set(result) != {"diagnostic", "line", "column"}:
                    raise ValueError(f"expected rejection for {case['id']}")
            elif result != {"encoded": case["expected"]}:
                raise ValueError(f"wrong encoded bytes for {case['id']}")
    shutil.copyfile(forbidden[-1], caller / "src/main.rs")
    inside("cargo", "build", "--locked", "--offline", "--release", cwd=caller)
    if inside(executable, cwd=caller) != native["TRAIT_OUTPUT"]:
        raise ValueError("unchanged native source-API witness differs")

    if input_digests() != inputs_before:
        raise ValueError("source wire contract, cases or target overlays changed during qualification")
    return {
        "format": "marmish-gap-range-native-idioms-v1",
        "framework": FRAMEWORK,
        "frameworkBuildToolchain": args.framework_toolchain,
        "manifestSha256": sha(MANIFEST),
        "companion": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "compilerSha256": sha(compiler),
        "node": node if args.typescript else None,
        "typescriptInputs": ts_inputs,
        "typescript": "5.9.3" if args.typescript else None,
        "verifierSha256": sha(HERE / "verify.py"),
        "contractVerifierSha256": sha(WORKSPACE_TOOLS["workspace_tool"]()[1]),
        "host": host,
        "rustc": version,
        "sealedPackageLockSha256": sha(reference_lock),
        "originalEndpointReceiptSha256": sha(HERE / f"{host}.json"),
        "wireContractSha256": sha(BUNDLE / "wire.json"),
        "casesSha256": sha(CASES),
        "hintsSha256": inputs_before[BUNDLE / "hints.json"],
        "bindingSha256": sha(BUNDLE / "rust.json"),
        "profiles": outputs,
        "privateBindingDiagnostic": diagnostic.replace(str(work), "<work>"),
        "observations": observations,
        "deniedReads": [path.name for path in forbidden],
        "wireProbeSha256": sha(BASELINE / "probe.rs"),
        "sourceApiProbeSha256": sha(forbidden[-1]),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--typescript", action="store_true")
    for name in ("framework", "compiler_target", "original", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    PRIOR["clean"](args.framework, FRAMEWORK)
    args.framework_toolchain = run("rustc", "+1.96.1", "-vV").decode().splitlines()
    if f"commit-hash: {PRIOR['BUILD_RUSTC']}" not in args.framework_toolchain:
        raise ValueError("expected the pinned Oddlang build toolchain")
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"):
        env.pop(key, None)
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", args.compiler_target, cwd=args.framework, env=env)
    args.compiler = args.compiler_target / "debug/oddlang"
    with tempfile.TemporaryDirectory(prefix="odd-endpoint-hints-") as temp:
        dump(args.receipt, verify(Path(temp).resolve(), args))
