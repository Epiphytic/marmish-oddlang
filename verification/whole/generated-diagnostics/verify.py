"""Rebuild all seven members and exercise the public diagnostics module outside its crate."""
import argparse
import json
from pathlib import Path
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BASE = "workspace-v7-offline-cli"
WORKSPACE = BASE + "-diagnostics"
DOMAIN = PORTABLE / "native-diagnostics-v1"
MEMBER = "marmish-native"
sys.path.insert(0, str(HERE.parent / "unified-mdk"))
try:
    SHARED = runpy.run_path(str(HERE.parent / "unified-mdk/verify.py"))
finally:
    sys.path.pop(0)
ORACLE = runpy.run_path(str(HERE.parent / "public-diagnostics/verify.py"))
run, data, sha = (SHARED[key] for key in ("run", "data", "sha"))


def check_product(product):
    lock = product / "workspace.lock"
    if lock.read_bytes() != (PORTABLE / WORKSPACE / "workspace.lock").read_bytes():
        raise ValueError("workspace lock differs from the release pin")
    parent = data(PORTABLE / BASE / "workspace.lock")
    current = data(lock)
    members = current["members"]
    if (members.keys() != parent["members"].keys() or current["cargoLockSha256"] != parent["cargoLockSha256"]
            or {k for k, v in members.items() if v != parent["members"][k]} != {MEMBER}):
        raise ValueError("unexpected workspace changes")
    parent_pin = HERE / "parent-package.lock"
    if sha(parent_pin) != parent["members"][MEMBER]["standalonePackageLockSha256"]:
        raise ValueError("incorrect parent native package lock")
    before = data(parent_pin)["generatedFiles"]
    after = data(product / MEMBER / "package.lock")["generatedFiles"]
    changed = {p for p in before.keys() | after.keys() if before.get(p) != after.get(p)}
    if changed != {"src/lib.rs", "src/group_9/mod.rs", "src/group_9/__projection_9.rs"}:
        raise ValueError(f"unexpected changed native source files: {sorted(changed)}")
    SHARED["check_member_files"](product, current)
    return current


def qualify(args, host, work, scratch):
    source = args.original / ORACLE["SOURCE"]
    baseline = data(HERE.parent / "public-diagnostics" / (host + ".json"))
    if baseline["original"] != SHARED["ORIGINAL"] or baseline["sourceSha256"] != sha(source):
        raise ValueError("different upstream diagnostic oracle")

    def observations(product):
        compiler = work / "oddlang"
        recovered = scratch / "recovered.json"
        run(compiler, "recover", "structured-diagnostics", source, recovered)
        if recovered.read_bytes() != (DOMAIN / "graph.json").read_bytes():
            raise ValueError("recovery differs from pinned graph")
        if baseline["probeSha256"] != sha(ORACLE["HERE"] / "probe.rs"):
            raise ValueError("different original consumer")
        library = product / "target/release/libmarmish_native.rlib"
        actual = ORACLE["observe"](library, host, "0.3.1", scratch)
        if actual != baseline["observations"]:
            raise ValueError("generated diagnostics differ from original")
        return {"diagnostics": {"consumerSha256": baseline["probeSha256"], "graphSha256": sha(recovered),
                                "originalLibrarySha256": baseline["librarySha256"], "librarySha256": sha(library),
                                "observations": actual}}

    return SHARED["qualify"](args, host, work, scratch, manifest=PORTABLE / (WORKSPACE + ".json"),
                            base=BASE + ".json", member=MEMBER, check=check_product,
                            extra_denied={"diagnostic-source": source, "diagnostic-consumer": ORACLE["HERE"] / "probe.rs"},
                            extra_oracle=observations, receipt_format="marmish-diagnostic-workspace-v1")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, required=True, type=lambda path: Path(path).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for path, pin in ((ROOT, None), (args.framework, args.framework_revision), (args.original, SHARED["ORIGINAL"])):
        if (run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)
                or pin and run("git", "rev-parse", "HEAD", cwd=path).decode().strip() != pin):
            raise ValueError(f"unexpected checkout: {path}")
    host, = (v[6:] for v in run("rustc", "+1.97.1", "-vV").decode().splitlines() if v.startswith("host: "))
    if host not in SHARED["PLATFORMS"]:
        raise ValueError(f"unqualified host: {host}")
    with tempfile.TemporaryDirectory() as w, tempfile.TemporaryDirectory() as s:
        receipt = qualify(args, host, Path(w).resolve(), Path(s).resolve())
    args.receipt.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
