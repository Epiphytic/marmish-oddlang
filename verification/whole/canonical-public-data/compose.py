"""Compose the source-recovered engine and native data over the canonical workspace."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib

PORTABLE = Path(__file__).resolve().parents[3] / "oddlang/portable"
BASE = "workspace-v7-canonical-offline"
OUTPUT = "workspace-v7-canonical-public-data"
ENGINE, NATIVE = "marmish-engine", "marmish-native"
ENGINE_PACKAGE = "marmish-engine-canonical-public-data-package.json"
OPTIONS = "engine-fieldless-choices-package.json"
REASON = "engine-gap-reason-package.json"
RECOVERED = "engine-gap-reason-recovered-v1"
STATUS = "workspace-v7-canonical-runtime-status.json"
CARGO = OUTPUT + "/Cargo.lock"
ENGINE_LOCK = OUTPUT + "/engine-Cargo.lock"


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def read(name):
    return json.loads((PORTABLE / name).read_bytes())


def pin(name):
    return {"path": name, "sha256": digest(PORTABLE / name)}


def check_pin(value):
    if pin(value["path"]) != value:
        raise ValueError(f"input digest changed: {value['path']}")
    return value["path"]


def lock_rows(raw):
    model = tomllib.loads(raw)
    if model.keys() != {"version", "package"} or model["version"] != 4:
        raise ValueError("unexpected Cargo lock format")
    rows = model["package"]
    return {(row["name"], row["version"], row.get("source")): row for row in rows}


def engine_closure(root):
    rows = lock_rows(root)
    by_name = {}
    for key in rows:
        by_name.setdefault(key[0], []).append(key)

    def resolve(reference):
        name, *qualifiers = reference.split(" ", 2)
        matches = by_name[name]
        if qualifiers:
            matches = [key for key in matches if key[1] == qualifiers[0]]
        if len(qualifiers) == 2:
            matches = [key for key in matches if f"({key[2]})" == qualifiers[1]]
        selected, = matches
        return selected

    pending = [resolve(ENGINE)]
    closure = {}
    while pending:
        key = pending.pop()
        if key in closure:
            continue
        row = rows[key]
        edges = [resolve(ref) for ref in row.get("dependencies", [])]
        if key[0] != ENGINE and not key[2].startswith("registry+"):
            raise ValueError("unexpected non-registry dependency in the engine lock")
        closure[key] = row, edges
        pending.extend(edges)
    names = [key[0] for key in closure]
    if len(names) != len(set(names)):
        raise ValueError("ambiguous engine dependency name")
    header, *_ = root.split("\n[[package]]")
    blocks = [header.rstrip()]
    for key, (row, edges) in sorted(closure.items()):
        if set(row) - {"name", "version", "source", "checksum", "dependencies"}:
            raise ValueError(f"unexpected Cargo package fields: {key}")
        block = ["[[package]]"]
        for name in ("name", "version", "source", "checksum"):
            if name in row:
                block.append(f"{name} = {json.dumps(row[name])}")
        if edges:
            block.extend(["dependencies = [", *[f' "{edge[0]}",' for edge in edges], "]"])
        blocks.append("\n".join(block))
    return "\n\n".join(blocks) + "\n"


def add_serde(raw):
    marker = '[[package]]\nname = "marmish-engine"\nversion = "0.1.0"\ndependencies = [\n "libc",\n]'
    if raw.count(marker) != 1:
        raise ValueError("unexpected prior engine dependency block")
    return raw.replace(marker, marker[:-1] + ' "serde",\n]', 1)


def compose(check=False):
    def output(name, blob):
        path = PORTABLE / name
        if check:
            if path.read_bytes() != blob:
                raise ValueError(f"composition drift: {name}")
        else:
            path.write_bytes(blob)
        return pin(name)

    def document(name, value):
        return output(name, (json.dumps(value, indent=2) + "\n").encode())

    base = read(BASE + ".json")
    choices = read(OPTIONS)
    engine = read(check_pin(base["members"][ENGINE]["manifest"]))
    choice = choices["components"][-1]
    if choices != {**engine, "components": [*engine["components"], choice]} or (
        choice["kind"], choice["module"], choice["reexports"]
    ) != ("checked-values", "model", ["Stream", "SubmitDisposition"]):
        raise ValueError("unexpected fieldless choice composition")

    reason = read(REASON)
    reason_component, = reason["components"]
    imported = dict(reason_component)
    for key, filename in (("graph", "graph.json"), ("overlay", "rust.json")):
        recovered = f"{RECOVERED}/{filename}"
        if read(check_pin(imported[key])) != read(recovered):
            raise ValueError("recovered MissingReason differs from qualified contract")
        imported[key] = pin(recovered)
    if (imported["kind"], imported["module"], imported["reexports"]) != (
        "shared-declarations", "model", ["MissingReason"]
    ) or reason["dependencies"].keys() != {"serde"}:
        raise ValueError("unexpected native MissingReason declaration")

    check_pin(engine["cargoLock"])
    check_pin(reason["cargoLock"])
    root = (PORTABLE / check_pin(base["cargoLock"])).read_text()
    combined = add_serde(root)
    prior, new = (lock_rows(raw) for raw in (root, combined))
    changed, = {key for key in new if new[key] != prior[key]}
    if changed[0] != ENGINE or new[changed]["dependencies"] != ["libc", "serde"] or new.keys() != prior.keys():
        raise ValueError("engine Serde cannot reuse the exact canonical Cargo registry closure")
    standalone = engine_closure(combined)

    engine_package = {
        **choices,
        "dependencies": {**engine["dependencies"], **reason["dependencies"]},
        "cargoLock": output(ENGINE_LOCK, standalone.encode()),
        "components": [*choices["components"], imported],
    }
    base["members"][ENGINE]["manifest"] = document(ENGINE_PACKAGE, engine_package)
    native_before = read(check_pin(base["members"][NATIVE]["manifest"]))
    native_profile = read(STATUS)["members"][NATIVE]
    native = read(check_pin(native_profile["manifest"]))
    index, = [i for i, row in enumerate(native_before["components"]) if row["module"] == "record_views"]
    expected = native_before["components"].copy()
    component = native["components"][index]
    if {key: val for key, val in component.items() if key not in ("graph", "overlay")} != {
        key: val for key, val in expected[index].items() if key not in ("graph", "overlay")
    }:
        raise ValueError("native status changed the journal module binding")
    expected[index] = component
    if native != {**native_before, "components": expected} or {
        key: val for key, val in native_profile.items() if key != "manifest"
    } != {key: val for key, val in base["members"][NATIVE].items() if key != "manifest"}:
        raise ValueError("native status candidate changed more than the qualified graph and overlay")
    base["members"][NATIVE] = native_profile
    base["cargoLock"] = output(CARGO, combined.encode())
    document(OUTPUT + ".json", base)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    compose(parser.parse_args().check)
