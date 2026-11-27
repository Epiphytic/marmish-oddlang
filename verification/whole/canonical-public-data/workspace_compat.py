"""Compatibility imports for the terminal and range verification programs."""
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WHOLE = HERE.parent
PORTABLE = ROOT / "oddlang/portable"
BASE, OUTPUT = "workspace-v7-canonical-offline", "workspace-v7-canonical-public-data"
ENGINE, NATIVE = "marmish-engine", "marmish-native"
FRAMEWORK = "900cfa517d0136fa80c2b5e92ccf9b1a3b2a71ab"
DARWIN, LINUX = "aarch64-apple-darwin", "aarch64-unknown-linux-gnu"
MANIFEST = PORTABLE / f"{OUTPUT}.json"
LOCK = PORTABLE / OUTPUT / "workspace.lock"
def lane(directory, script="verify.py"):
    path = WHOLE / directory
    prior_path = sys.path.copy()
    modules = {key: sys.modules.pop(key) for key in ("compose", "verify") if key in sys.modules}
    sys.path.insert(0, str(path))
    try:
        return runpy.run_path(str(path / script))
    finally:
        sys.path[:] = prior_path
        for key in ("compose", "verify"):
            sys.modules.pop(key, None)
        sys.modules.update(modules)


SIX = lane("unified-mdk", "workspace_compat.py")
JOURNAL = SIX["JOURNAL"]
PREVIOUS = lane("canonical-linux-imports")
AUDIT = PREVIOUS["audit"]
ENGINE_DATA = lane("engine-fieldless-choices")
run, data, digest = SIX["run"], SIX["data"], SIX["sha"]


def changed_files(before, after):
    first, last = (data(package / "package.lock")["generatedFiles"] for package in (before, after))
    if first.keys() - last.keys():
        raise ValueError("qualified files disappeared")
    return {name for name in first if first[name] != last[name]}, set(last) - set(first)


def dependency(rows, name, target_release):
    path, = {
        Path(filename)
        for row in rows if row["reason"] == "compiler-artifact" and row["target"]["name"] == name
        and row["target"]["kind"] == ["lib"] and row["profile"]["opt_level"] == "3"
        for filename in row["filenames"]
        if filename.endswith(".rlib") and Path(filename).is_relative_to(target_release)
    }
    return path


def domains(host, raw, target, roles, links, work):
    rows = [json.loads(line) for line in raw.splitlines()]
    release = target / host / "release"
    return SIX["product_witness"]("domains", current={
        "host": host, "work": work, "target": target, "roles": roles, "links": links,
        "serde": dependency(rows, "serde", release),
        "serdeJson": dependency(rows, "serde_json", release),
    })
