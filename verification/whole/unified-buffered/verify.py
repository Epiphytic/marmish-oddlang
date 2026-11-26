"""Qualify the seven-member sealed workspace and its private input reader."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

from compose import BASE, MEMBER, WORKSPACE, compose

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / WORKSPACE
PRIVATE = HERE.parent / "private-buffer"


def load_lane(path, *, isolated_modules=()):
    saved = {name: sys.modules.pop(name) for name in isolated_modules if name in sys.modules}
    sys.path.insert(0, str(path.parent))
    try:
        return runpy.run_path(str(path))
    finally:
        sys.path.pop(0)
        for name in isolated_modules:
            sys.modules.pop(name, None)
        sys.modules.update(saved)


SIX = load_lane(HERE.parent / "unified-mdk/verify.py", isolated_modules=("compose",))
BUFFER = load_lane(PRIVATE / "verify.py")
run, data, sha = (SIX[key] for key in ("run", "data", "sha"))


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (MANIFEST.with_suffix("") / "workspace.lock").read_bytes():
        raise ValueError("seven-member projection lock mismatch")
    current = data(lock)
    previous = data((PORTABLE / BASE).with_suffix("") / "workspace.lock")
    members = dict(current["members"])
    new = members.pop(MEMBER)
    if members != previous["members"] or len(members) != 6 or new["dependencies"]:
        raise ValueError("existing workspace member pins, sibling edges or new member changed")
    SIX["check_member_files"](product, current)
    return current


def buffer_oracle(product, original, host):
    witness = data(PRIVATE / f"original-{host}.json")
    cases = [BUFFER["ORIGINAL_CASE"]] + [BUFFER["PREFIX"] + name for name in BUFFER["CASES"]]
    source = run("git", "-C", original, "show", f'{BUFFER["PIN"]}:{BUFFER["SOURCE"]}')
    if (witness["revision"] != BUFFER["PIN"] or witness["target"] != host
            or witness["passed"] != cases or witness["upstreamFileSha256"] != BUFFER["sha"](source)
            or witness["callerSha256"] != sha(PRIVATE / "observations.rs")):
        raise ValueError("private buffer witnesses differ from the original baseline")
    provider = BUFFER["erasure_provider"](original, host)
    leaf = product / MEMBER / "src/group_0/__projection_0.rs"
    sealed = leaf.read_bytes()
    baseline = data(PRIVATE / f"candidate-{host}.json")
    package_hash = sha(product / MEMBER / "package.lock")
    if (baseline["baseline"]["packageLockSha256"] != package_hash
            or baseline["erasureProvider"] != provider):
        raise ValueError("sealed private reader differs from standalone qualification")
    try:
        leaf.write_bytes(sealed + BUFFER["original_test"](original) + (PRIVATE / "observations.rs").read_bytes())
        for case in cases:
            exact = BUFFER["NATIVE_TEST_PREFIX"] + case
            result = subprocess.run(
                ["cargo", "+1.97.1", "test", "--release", "--locked", "--offline", "--lib",
                 "-p", MEMBER, "--", "--exact", exact], cwd=product,
                env=SIX["JOURNAL"]["cargo_env"](), capture_output=True, text=True,
            )
            if result.returncode or result.stdout.splitlines().count(f"test {exact} ... ok") != 1:
                raise ValueError(result.stdout + result.stderr)
    finally:
        leaf.write_bytes(sealed)
    return {"bufferedInput": {
        "scope": "Private in-memory buffered reader",
        "standalonePackageLockSha256": package_hash,
        "originalWitnessSha256": sha(PRIVATE / f"original-{host}.json"),
        "standaloneGeneratedWitnessSha256": sha(PRIVATE / f"candidate-{host}.json"),
        "observationsSha256": sha(PRIVATE / "observations.rs"),
        "erasureProvider": provider,
        "passed": cases,
    }}


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
    with tempfile.TemporaryDirectory(prefix="odd-seven-input-") as work, tempfile.TemporaryDirectory(
        prefix="odd-seven-oracles-"
    ) as scratch:
        receipt = SIX["qualify"](
            args, host, Path(work).resolve(), Path(scratch).resolve(),
            manifest=MANIFEST, base=BASE, member=MEMBER, check=check_product,
            extra_denied={"original-prototype": args.original / BUFFER["SOURCE"],
                          "buffer-caller": PRIVATE / "observations.rs"},
            extra_oracle=lambda product: buffer_oracle(product, args.original, host),
            receipt_format="marmish-seven-member-private-buffer-v1",
        )
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
