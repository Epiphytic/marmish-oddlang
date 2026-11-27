"""Build the checked range binding in a sealed workspace and compare with the qualified product."""
import argparse
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import sys

from compose import BASE, ENGINE, HERE, PACKAGE, PORTABLE, ROLE_BINDING, ROOT, WORKSPACE, compose, pin

WHOLE = HERE.parent
SCRIPT = WHOLE / "canonical-recovered-range/workspace_compat.py"
saved = sys.modules.pop("compose")
sys.path.insert(0, str(SCRIPT.parent))
try:
    BASELINE = runpy.run_path(str(SCRIPT))
finally:
    sys.path.pop(0)
    sys.modules["compose"] = saved

FRAMEWORK = "b530e740a6eaef5e993192777463bf39e222c477"
ASSEMBLY, AUDIT, JOURNAL = (BASELINE[key] for key in ("ASSEMBLY", "AUDIT", "JOURNAL"))
data, digest, hashed, tree, delta = (BASELINE[key] for key in ("data", "sha", "hashed", "tree", "lock_delta"))


def verify(args):
    compose(check=True)
    version = subprocess.check_output(["rustc", "+1.97.1", "-vV"], text=True).splitlines()
    host, = [line[6:] for line in version if line.startswith("host: ")]
    if host not in (BASELINE["DARWIN"], BASELINE["LINUX"]) or f"commit-hash: {AUDIT.COMPILER}" not in version:
        raise ValueError("requires the pinned native ARM64 toolchain")
    subprocess.run(["git", "-C", args.framework, "diff", "--exit-code", "HEAD"], check=True)
    if subprocess.check_output(["git", "-C", args.framework, "rev-parse", "HEAD"], text=True).strip() != FRAMEWORK:
        raise ValueError("wrong framework revision")
    host_path = f"{host}.json"
    prior_path = WHOLE / "canonical-recovered-range" / host_path
    endpoint_path = WHOLE / "gap-range-endpoint" / host_path
    prior, endpoint = data(prior_path), data(endpoint_path)
    before = tree(args.baseline)
    if hashed(before) != prior["productComparison"]["recoveredTreeSha256"] or (
        endpoint["sourceEquivalenceReceiptSha256"] != digest(prior_path)
        or endpoint["generatedBuildReceiptSha256"] != prior["priorNativeReceiptSha256"]
        or endpoint["generatedWorkspaceLockSha256"] != prior["priorWorkspaceLockSha256"]
        or endpoint["casesSha256"] != digest(endpoint_path.with_name("cases.json"))
    ):
        raise ValueError("recovered predecessor or exact native endpoint evidence differs")

    args.work.mkdir()
    inputs = args.work / "inputs"
    inputs.mkdir()
    manifest = PORTABLE / f"{WORKSPACE}.json"
    spec = ASSEMBLY["stage_workspace"](manifest, inputs)
    staged = {p.relative_to(inputs).as_posix(): digest(p) for p in sorted(inputs.rglob("*")) if p.is_file()}
    prior_staged = prior["stagedInputs"]
    old_package = data(PORTABLE / f"{BASE}.json")["members"][ENGINE]["manifest"]["path"]
    old_overlay = "engine-gap-range-recovered-v1/rust.json"
    removed, added = {f"{BASE}.json", old_package, old_overlay}, {manifest.name, PACKAGE, ROLE_BINDING}
    if set(prior_staged) - set(staged) != removed or set(staged) - set(prior_staged) != added or any(
        prior_staged[name] != staged[name] for name in prior_staged.keys() & staged.keys()
    ) or (inputs / ROLE_BINDING).read_bytes() != (PORTABLE / ROLE_BINDING).read_bytes():
        raise ValueError("unexpected sealed input changes for the role binding")

    compiler = Path(shutil.copy2(args.compiler, args.work / "oddlang"))
    compiler_sha = digest(compiler)
    cache = args.work / "providers"
    ASSEMBLY["stage_provider_git_cache"](PORTABLE / spec["cargoLock"]["path"], cache)
    prefix, env = JOURNAL["sandbox"](ASSEMBLY, args.work, (ROOT, args.framework, args.baseline, args.compiler), cache)
    for path in (PORTABLE / old_overlay, args.framework / "src/lib.rs", args.baseline / ENGINE / "src/lib.rs"):
        denied = subprocess.run([*prefix, "/bin/cat", path], env=env, cwd=args.work, capture_output=True)
        if not path.is_file() or denied.returncode != 1 or denied.stdout or not any(
            issue in denied.stderr for issue in (b"Operation not permitted", b"No such file or directory")
        ):
            raise ValueError(f"generation sandbox did not deny {path}")
    product = args.work / "candidate"
    subprocess.run([*prefix, compiler, "build", "native-workspace", "rust", inputs / manifest.name, product],
                   env=env, cwd=args.work, check=True)
    if digest(compiler) != compiler_sha:
        raise ValueError("compiler changed during generation")
    package_lock = f"{ENGINE}/package.lock"
    root_lock = "workspace.lock"
    after = tree(product)
    changed = {path for path in before.keys() & after.keys() if before[path] != after[path]}
    if before.keys() != after.keys() or changed != {root_lock, package_lock}:
        raise ValueError(f"emitted layout or file bytes changed: {sorted(changed)}")
    if any((product / name).read_bytes() != (args.baseline / name).read_bytes()
           for name, row in before.items() if row["kind"] == "file" and name not in changed):
        raise ValueError("generated non-lock bytes differ from the qualified subject")
    leaf = f"{ENGINE}/src/group_1/__projection_5.rs"
    if after[leaf]["sha256"] != prior["productComparison"]["identicalRustFiles"][leaf]:
        raise ValueError("generated range differs from native-qualified Rust")

    pins = {name: delta(data(args.baseline / name), data(product / name)) for name in sorted(changed)}
    expected_workspace = {"inputSha256", f"members/{ENGINE}/inputSha256", f"members/{ENGINE}/standalonePackageLockSha256"}
    expected_package = {"manifestSha256", "components/5/overlaySha256", "components/5/projection/overlaySha256"}
    if pins[root_lock].keys() != expected_workspace or pins[package_lock].keys() != expected_package or (
        pins[package_lock]["components/5/overlaySha256"]["after"] != pin(ROLE_BINDING)["sha256"]
    ):
        raise ValueError(f"unexpected workspace provenance deltas: {pins}")
    frozen = PORTABLE / WORKSPACE / root_lock
    if frozen.read_bytes() != (product / root_lock).read_bytes():
        raise ValueError("assembled workspace lock does not match committed lock")
    common = {name: row for name, row in after.items() if name not in changed}
    return {
        "host": host, "rustc": version, "framework": FRAMEWORK, "compilerSha256": compiler_sha,
        "manifestSha256": digest(manifest), "engineManifestSha256": pin(PACKAGE)["sha256"],
        "roleBindingSha256": pin(ROLE_BINDING)["sha256"], "workspaceLockSha256": digest(product / root_lock),
        "predecessorReceiptSha256": digest(prior_path), "nativeEndpointReceiptSha256": digest(endpoint_path),
        "stagedInputs": len(staged), "stagedInputsSha256": hashed(staged), "deniedInputs": 3,
        "files": sum(r["kind"] == "file" for r in after.values()),
        "directories": sum(r["kind"] == "directory" for r in after.values()),
        "identicalRustFiles": sum(name.endswith(".rs") for name in common),
        "identicalCommonTreeSha256": hashed(common), "rangeRustSha256": after[leaf]["sha256"],
        "lockDeltas": pins,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler", "baseline", "work", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    args.receipt.write_text(json.dumps(verify(args), indent=2, sort_keys=True) + "\n")
