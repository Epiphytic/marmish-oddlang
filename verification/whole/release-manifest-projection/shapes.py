"""Normalize the selected Rustdoc-declared record field and trait identities."""
from pathlib import Path
import json

TRAITS = {
    ("core", "clone", "Clone"): "clone",
    ("core", "fmt", "Debug"): "debug",
    ("core", "cmp", "PartialEq"): "partial-eq",
    ("core", "cmp", "Eq"): "eq",
    ("core", "marker", "StructuralPartialEq"): None,
}
NATIVE = {"Manifest", "Asset"}


def shape(path, filename):
    d = json.loads(Path(path).read_bytes())
    if d["format_version"] != 57:
        raise ValueError("expected Rustdoc JSON 57")
    index = d["index"]
    paths = d["paths"]

    def item(id):
        return index[str(id)]

    def native_path(value):
        return tuple(paths[str(value["id"])]["path"])

    found = {}
    for record in index.values():
        if record["crate_id"] or record["name"] not in NATIVE or (
            (record["span"] or {}).get("filename") != filename
        ):
            continue
        if record["name"] in found or record["visibility"] != "crate":
            raise ValueError("expected one crate-visible declaration of each selected record")
        internal = record["inner"]["struct"]
        plain = internal["kind"]["plain"]
        if plain["has_stripped_fields"] or any(internal["generics"].values()):
            raise ValueError("expected full named record with no generic parameters")
        found[record["name"]] = (record, internal)
    if set(found) != NATIVE:
        raise ValueError("source document omits a selected record")

    def type_shape(ty):
        if "primitive" in ty:
            name = ty["primitive"]
            if name == "bool":
                return {"kind": "boolean"}
            if name in ("u8", "u16", "u32", "u64"):
                return {"kind": "unsigned", "bits": int(name[1:])}
        if "borrowed_ref" in ty and ty["borrowed_ref"] == {
            "lifetime": "'static", "is_mutable": False, "type": {"primitive": "str"}
        }:
            return {"kind": "static-text"}
        if "array" in ty:
            element = type_shape(ty["array"]["type"])
            extent = int(ty["array"]["len"])
            if element == {"kind": "unsigned", "bits": 8}:
                return {"kind": "bytes", "length": extent}
            return {"kind": "unsigned-array", "bits": element["bits"], "length": extent}
        if "resolved_path" in ty:
            resolved = ty["resolved_path"]
            route = native_path(resolved)
            if route == ("alloc", "string", "String") and resolved["args"] is None:
                return {"kind": "owned-text"}
            if route == ("alloc", "vec", "Vec"):
                args = resolved["args"]["angle_bracketed"]
                arg, = args["args"]
                element = arg["type"]["resolved_path"]
                if args["constraints"] or element["args"] is not None:
                    raise ValueError("unexpected vector generic parameters")
                row = item(element["id"])
                if row["name"] not in found or found[row["name"]][0]["id"] != row["id"]:
                    raise ValueError("vector item does not identify a selected local record")
                return {"kind": "record-vector", "record": row["name"]}
        raise ValueError(f"unsupported compiler-observed field type {ty!r}")

    result = {}
    for name, (record, internal) in found.items():
        fields = []
        for field in internal["kind"]["plain"]["fields"]:
            f = item(field)
            if f["visibility"] != "public":
                raise ValueError("unexpected field visibility")
            fields.append({"name": f["name"], "type": type_shape(f["inner"]["struct_field"])})
        derives = []
        methods = []
        for member in internal["impls"]:
            implementation = item(member)
            body = implementation["inner"]["impl"]
            if body["is_synthetic"] or body["blanket_impl"] is not None:
                continue
            if body["trait"] is None:
                methods.extend(
                    {"name": item(method)["name"], "visibility": item(method)["visibility"]}
                    for method in body["items"]
                )
                continue
            if "automatically_derived" not in implementation["attrs"]:
                raise ValueError("unexpected authored implementation in scoped record")
            normalized = TRAITS[native_path(body["trait"])]
            if normalized:
                derives.append(normalized)
        result[name] = {
            "visibility": record["visibility"],
            "fields": fields,
            "derives": sorted(derives),
            "methods": sorted(methods, key=lambda method: method["name"]),
        }
    return result


def declared(graph, overlay):
    results = graph["recordResult"]
    records = overlay["records"]
    observed = {}
    for id, fields in results["records"].items():
        binding = records[id]
        native = []
        for field in fields:
            mapped = dict(field["type"])
            if mapped["kind"] == "record-vector":
                mapped["record"] = records[mapped["record"]]["name"]
            native.append({"name": binding["fields"][field["id"]], "type": mapped})
        api = []
        if results["entry"]["record"] == id:
            api = sorted([overlay["parser"], *overlay["formatters"].values()])
        observed[binding["name"]] = {
            "visibility": overlay["visibility"], "fields": native,
            "derives": sorted(binding["derives"]),
            "methods": [{"name": name, "visibility": overlay["visibility"]} for name in api],
        }
    return observed
