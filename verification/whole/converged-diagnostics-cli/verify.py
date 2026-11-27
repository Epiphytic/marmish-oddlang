"""Probe CLI and public imports from the same isolated seven-member build."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
CLI = "workspace-v7-offline-cli"
DIAGNOSTICS = "workspace-v7-offline-cli-diagnostics"
OUTPUT = "workspace-v7-converged-diagnostics-cli"
PUBLIC = "workspace-v7-converged-public-api"

ROOT = HERE.parents[2]
FRAMEWORK = "0568396f8d0641b266aed1702d482d71c79b3ba4"
HOST = "aarch64-apple-darwin"
ROLES = ("marmish", "marmishd")


def lane(name, filename):
    directory = HERE.parent / name
    own_compose = sys.modules.pop("compose", None)
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / filename))
    finally:
        sys.path.pop(0)
        sys.modules.pop("compose", None)
        if own_compose is not None:
            sys.modules["compose"] = own_compose


WORKSPACE = lane("workspace", "verify.py")
IMPORTS = lane("seven-public-imports", "verify.py")
CLI_CASES = json.loads((HERE.parent / "offline-cli/cases.json").read_bytes())
SIX = lane("unified-mdk", "workspace_compat.py")
IDENTITY = lane("backend-choice", "verify.py")


load = SIX["data"]
sha = SIX["JOURNAL"]["sha"]


def observe_cli(binary, argv, home):
    return SIX["product_witness"]("offline-cli-case", binary, home, "--", *argv)


def check_composition():
    SIX["check_api_release"]("converged-diagnostics-cli", PORTABLE)


def execute(*args, cwd=None, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, timeout=1600)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result.stdout


def exact_checkout(root, revision):
    if (execute("git", "-C", root, "rev-parse", "HEAD").decode().strip() != revision
            or execute("git", "-C", root, "status", "--porcelain", "--untracked-files=all")):
        raise ValueError(f"requires a clean checkout at {revision}: {root}")


def build_compiler(args):
    if any(args.compiler_target.is_relative_to(root) for root in (ROOT, args.framework, args.original)):
        raise ValueError("use an external compiler target directory")
    execute("cargo", "+1.97.1", "build", "--release", "--offline", "--locked",
            "--bin", "oddlang", "--target-dir", args.compiler_target, cwd=args.framework)
    return args.compiler_target / "release/oddlang"


def build(args, scratch, profile, executable):
    manifest = PORTABLE / f"{OUTPUT}.json"
    inputs = scratch / "inputs"
    inputs.mkdir()
    WORKSPACE["stage_workspace"](manifest, inputs)
    compiler = shutil.copy2(executable, scratch / "oddlang")
    denied = (ROOT, args.original, args.framework)
    policy = "(version 1) (allow default) (deny network*)" + "".join(
        f" (deny file-read* (subpath {json.dumps(str(path))}))" for path in denied)
    sandbox = ("/usr/bin/sandbox-exec", "-p", policy)
    for probe in (manifest, args.original / "Cargo.toml", args.framework / "Cargo.toml"):
        if subprocess.run([*sandbox, "/bin/cat", probe], capture_output=True).returncode == 0:
            raise ValueError(f"isolation unexpectedly read {probe}")
    execute(*sandbox, compiler, "build", "native-workspace", "rust", inputs / manifest.name, args.product,
            env={**os.environ, "RUSTUP_TOOLCHAIN": "1.97.1"})
    IMPORTS["verify_product"](args.product, ROOT, profile)
    raw = execute(*sandbox, "cargo", "+1.97.1", "build", "--release", "--workspace", "--lib", "--bins",
                  "--locked", "--offline", "--target-dir", args.target, "--message-format=json", cwd=args.product)
    IMPORTS["verify_product"](args.product, ROOT, profile)
    return raw


def binaries(raw, product, target):
    result = {}
    for line in raw.splitlines():
        row = json.loads(line)
        if row["reason"] != "compiler-artifact" or row["target"]["name"] not in ROLES:
            continue
        name = row["target"]["name"]
        if (row["target"]["kind"] != ["bin"] or row["profile"]["test"] or row["profile"]["opt_level"] != "3"
                or Path(row["manifest_path"]).resolve() != product / "marmish-native/Cargo.toml"
                or Path(row["executable"]).resolve() != target / "release" / name or name in result):
            raise ValueError(f"unexpected binary artifact for {name}")
        result[name] = Path(row["executable"])
    if result.keys() != set(ROLES):
        raise ValueError("both released binary roles must be present")
    return result


def cli_observations(args, generated, scratch):
    captured = load(HERE.parent / "offline-cli" / f"{HOST}.json")
    old = load(HERE.parent / "cli/transcript.json")
    output = {}
    for name, binary in generated.items():
        original = args.original_bin_dir / name
        if sha(original.read_bytes()) != captured["binarySha256"]["original"][name]:
            raise ValueError(f"original CLI pin differs for {name}")
        expected = [(row, row) for row in old[name]]
        expected += [(row["original"], row["successor"]) for row in captured["rows"][name]]
        historical = {tuple(row["args"]) for row in old[name]}
        historic_safe = {("--help",), ("--version",), ("--oddlang-unrecognized-option",)}
        home = scratch / name
        home.mkdir()
        for original_row, successor_row in expected:
            argv = original_row["args"]
            tokens = tuple(argv)
            permitted_history = tokens in historic_safe or (
                len(argv) == 2 and argv[0] == "help" and argv[1].isascii() and argv[1].replace("-", "").isalnum())
            if original_row["args"] != successor_row["args"] or (
                    tokens not in historical and argv not in CLI_CASES
                    or tokens in historical and not permitted_history):
                raise ValueError(f"unrecognized fixed replay case: {argv!r}")
            orig = observe_cli(original, argv, home)
            candidate = observe_cli(binary, argv, home)
            if orig != original_row or candidate != successor_row:
                raise ValueError(f"changed CLI bytes for {name} {argv!r}")
        if any(home.iterdir()):
            raise ValueError("offline CLI changed its working directory")
        output[name] = {"cases": len(expected), "matchesOriginal": sum(a == b for a, b in expected),
                        "uncoveredArguments": [a["args"] for a, b in expected if a != b],
                        "generatedSha256": sha(binary.read_bytes()), "originalSha256": sha(original.read_bytes())}
    return output


def imports_and_behavior(args, raw, scratch):
    packages = [f"marmish-{key}" for key in IMPORTS["COUNTS"]]
    found, links = IMPORTS["artifacts"](raw, args.product, packages, args.target, generated=True)
    observed = IMPORTS["library_imports"](found, args.original)
    previous = load(HERE.parent / "converged-public-api/darwin-imports.json")["imports"]
    delta = []
    for crate, result in observed.items():
        before = {row["spelling"]: row["compiles"] for row in previous[crate]["imports"]}
        after = {row["spelling"]: row["compiles"] for row in result["imports"]}
        if before.keys() != after.keys() or any(before[key] and not after[key] for key in before):
            raise ValueError(f"lost inherited public import in {crate}")
        delta.extend((crate, key) for key in sorted(after) if after[key] and not before[key])
    if delta != [("native", "diagnostics")] or sum(v["importable"] for v in observed.values()) != 71:
        raise ValueError(f"unexpected import delta: {delta!r}")

    artifact = SIX["artifact"]
    ref_engine = SIX["cargo_reference"](args, "marmish-engine")
    ref_native = SIX["cargo_reference"](args, "marmish-native")
    ref_prototype_raw = execute("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--lib",
                                "-p", "marmish-prototype", "--target", HOST, "--target-dir", args.original_target,
                                "--message-format=json", cwd=args.original)
    ref_prototype, ref_links = IMPORTS["artifacts"](ref_prototype_raw, args.original, ["marmish-prototype"],
                                                  args.original_target, generated=False)
    constant = IMPORTS["prototype_probe"](ref_prototype["marmish-prototype"], found["marmish-prototype"],
                                           ref_links, links)
    if constant["generated"]["MAX_LINE"].get("value") != 131072 or constant["generated"]["serve"]["compiles"]:
        raise ValueError("prototype public probes regressed")

    engine, serde = artifact(ref_engine, "marmish_engine"), artifact(ref_engine, "serde")
    original_engine = SIX["product_witness"]("engine-caller", "errors", engine, engine.parent / "deps", serde, scratch / "original-engine")
    generated_engine = SIX["product_witness"]("engine-caller", "errors", found["marmish-engine"][1], found["marmish-engine"][0].parent,
                                          artifact(raw, "serde"), scratch / "generated-engine")
    native, serde = artifact(ref_native, "marmish_native"), artifact(ref_native, "serde")
    choice = IDENTITY["observe"]
    original_identity = choice(native, native.parent / "deps", serde, serde.parent, scratch / "original-identity")
    candidate_serde = artifact(raw, "serde")
    generated_identity = choice(found["marmish-native"][1], found["marmish-native"][0].parent,
                                candidate_serde, candidate_serde.parent, scratch / "generated-identity")
    if original_engine != generated_engine or original_identity != generated_identity:
        raise ValueError("inherited engine or identity public behavior changed")
    diag = load(HERE.parent / "public-diagnostics" / f"{HOST}.json")
    probe = HERE.parent / "public-diagnostics/probe.rs"
    if (diag["original"] != IMPORTS["ORIGINAL"] or diag["probeSha256"] != sha(probe.read_bytes())
            or diag["sourceSha256"] != sha((args.original / "crates/marmish-native/src/diagnostics.rs").read_bytes())):
        raise ValueError("diagnostic baseline pin differs")
    generated_diag = SIX["product_witness"]("diagnostic-caller", found["marmish-native"][1], HOST, "0.3.1", scratch)
    if generated_diag != diag["observations"]:
        raise ValueError("public diagnostic behavior differs")
    return {"spellingCount": 140, "importable": 71, "delta": delta, "libraries": observed,
            "prototype": constant, "engine": {"outputs": generated_engine[0], "negative": generated_engine[1]},
            "identity": {"outputs": generated_identity[0].decode(), "negative": generated_identity[1]},
            "diagnostics": generated_diag}


def verify(args):
    exact_checkout(ROOT, args.input_revision)
    exact_checkout(args.original, IMPORTS["ORIGINAL"])
    exact_checkout(args.framework, FRAMEWORK)
    if (args.target.is_relative_to(args.product) or args.target.is_relative_to(args.original)
            or args.original_target.is_relative_to(args.original)):
        raise ValueError("use an external target directory")
    rustc = execute("rustc", "+1.97.1", "-vV").decode().splitlines()
    if f"host: {HOST}" not in rustc or f"commit-hash: {IMPORTS['COMPILER']}" not in rustc:
        raise ValueError("requires a native Darwin ARM64 toolchain")
    check_composition()
    lock = PORTABLE / OUTPUT / "workspace.lock"
    profile = IMPORTS["Candidate"](OUTPUT, args.input_revision, FRAMEWORK, sha(lock.read_bytes()))
    parent_locks = {name: load(PORTABLE / name / "workspace.lock") for name in (PUBLIC, CLI, DIAGNOSTICS)}
    current = load(lock)
    for name, member in parent_locks[PUBLIC]["members"].items():
        if name != "marmish-native" and current["members"].get(name) != member:
            raise ValueError(f"inherited member lock changed: {name}")
    for parent in parent_locks.values():
        if any(parent[key] != current[key] for key in ("cargoLockSha256", "workspaceCargoManifestSha256")):
            raise ValueError("shared Cargo lock or root manifest changed")
    compiler = build_compiler(args)
    with tempfile.TemporaryDirectory(prefix="odd-converged-diagnostics-") as temporary:
        scratch = Path(temporary).resolve()
        raw = build(args, scratch, profile, compiler)
        cli = cli_observations(args, binaries(raw, args.product, args.target), scratch)
        public_api = imports_and_behavior(args, raw, scratch)
        IMPORTS["verify_product"](args.product, ROOT, profile)
    return {"platform": HOST, "frameworkRevision": FRAMEWORK, "originalRevision": IMPORTS["ORIGINAL"],
            "workspaceInputRevision": args.input_revision, "workspaceLockSha256": profile.lock_sha256,
            "compilerSha256": sha(compiler.read_bytes()), "cargoArtifactLogSha256": sha(raw),
            "rustc": rustc, "verifierSha256": sha(Path(__file__).read_bytes()),
            "parents": {name: sha((PORTABLE / name / "workspace.lock").read_bytes()) for name in parent_locks},
            "cli": cli, "publicApi": public_api}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_revision")
    for name in ("compiler_target", "framework", "original", "original_bin_dir", "original_target", "product", "target", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    parsed = parser.parse_args()
    result = verify(parsed)
    parsed.receipt.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
