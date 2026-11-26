"""Combine independently qualified, dependency-preserving native components."""
import argparse
import runpy
from pathlib import Path

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
BASE = "workspace-v7-converged-diagnostics-cli"
PUBLIC = "workspace-v7-converged-public-api"
PREFIX = "workspace-v7-terminal-prefix"
PARSER = "workspace-v7-converged-release-manifest"
RECORDS = "workspace-v7-observational-records"
OUTPUT = "workspace-v7-canonical-offline"
NATIVE = "marmish-native"
ENGINE = "marmish-engine"
VAULT = "marmish-vault"
VAULT_MODE = "vault-fixed-value-mode-package.json"

COMMON = runpy.run_path(str(HERE.parent / "converged-diagnostics-cli/compose.py"))
load, encode, sha, pin, checked, components, combine = (
    COMMON[name] for name in ("load", "encode", "sha", "pin", "checked_package", "components", "combine_packages")
)


def same_metadata(a, b, ignored):
    return {k: v for k, v in a.items() if k != ignored} == {k: v for k, v in b.items() if k != ignored}


def ensure_workspace(parent, child, changed):
    if not same_metadata(parent, child, "members") or parent["members"].keys() != child["members"].keys():
        raise ValueError("a composition input changes shared workspace structure")
    for name, member in parent["members"].items():
        other = child["members"][name]
        if not same_metadata(member, other, "manifest") or (name not in changed and member != other):
            raise ValueError(f"unexpected workspace member or dependency edit: {name}")


def package(root, workspace, member):
    return checked(root, workspace["members"][member]["manifest"])


def append_only(original, successor, expected):
    if not same_metadata(original, successor, "components"):
        raise ValueError("only native components may be appended")
    former, current = components(original), components(successor)
    if (list(current) != [*former, *expected] or
            any(current[k] != v for k, v in former.items())):
        raise ValueError(f"unexpected native addition or replacement: {expected}")
    return [current[key] for key in expected]


def prefix_updates(base, prefix):
    if not same_metadata(base, prefix, "components"):
        raise ValueError("terminal prefix input changes native metadata")
    previous, selected = components(base), components(prefix)
    if list(previous) != list(selected):
        raise ValueError("terminal prefix input changes the native component order")
    affected = {("cli-surface", "marmish_parser"), ("cli-surface", "marmishd_parser")}
    changed = {key for key, value in selected.items() if value != previous[key]}
    if changed != affected:
        raise ValueError("terminal prefix input must replace the two established CLI graphs")
    for key in affected:
        a, b = previous[key], selected[key]
        if not same_metadata(a, b, "graph") or b["graph"]["path"] != "native-cli-prefix-v1/graph.json":
            raise ValueError(f"unexpected terminal CLI binding edit for {key}")


def compose(root, check=False):
    root = root.resolve()
    public = load(root / (PUBLIC + ".json"))
    base = load(root / (BASE + ".json"))
    prefix = load(root / (PREFIX + ".json"))
    parser = load(root / (PARSER + ".json"))
    records = load(root / (RECORDS + ".json"))
    ensure_workspace(public, base, {NATIVE})
    ensure_workspace(base, prefix, {NATIVE})
    ensure_workspace(public, parser, {NATIVE})
    ensure_workspace(public, records, {ENGINE, NATIVE})
    current_native = package(root, base, NATIVE)
    prefix_native = package(root, prefix, NATIVE)
    parser_native = package(root, parser, NATIVE)
    record_native = package(root, records, NATIVE)
    common_native = package(root, public, NATIVE)
    prefix_updates(current_native, prefix_native)
    append_only(common_native, parser_native, [("line-grammar", "native_manifest")])
    append_only(common_native, record_native, [("public-records", "transport")])
    added = combine(common_native, [parser_native, record_native])["components"][len(common_native["components"]):]
    known = components(prefix_native)
    if any((component["kind"], component["module"]) in known for component in added):
        raise ValueError("new native definitions collide with a CLI or diagnostic component")
    native = dict(prefix_native, components=prefix_native["components"] + added)

    engine = package(root, records, ENGINE)
    append_only(package(root, public, ENGINE), engine, [("public-records", "model")])
    mode_path = root / VAULT_MODE
    mode = load(mode_path)
    append_only(package(root, base, VAULT), mode, [("checked-values", "helper")])

    native_path = root / "marmish-native-canonical-offline-package.json"
    native_bytes = encode(native)
    members = base["members"].copy()
    members[NATIVE] = dict(members[NATIVE], manifest=pin(root, native_path, native_bytes))
    members[ENGINE] = records["members"][ENGINE]
    members[VAULT] = dict(members[VAULT], manifest=pin(root, mode_path, mode_path.read_bytes()))
    output = root / (OUTPUT + ".json")
    expected = {native_path: native_bytes, output: encode(dict(base, members=members))}
    for path, content in expected.items():
        if check:
            if path.read_bytes() != content:
                raise ValueError(f"canonical composition differs: {path}")
        else:
            path.write_bytes(content)
    return output


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("portable", type=Path, nargs="?", default=PORTABLE)
    cli.add_argument("--check", action="store_true")
    args = cli.parse_args()
    compose(args.portable, args.check)
