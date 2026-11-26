"""Compare array shapes with the previous compiler graph and an independent rustdoc projection."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def walk(value):
    yield value
    children = value.values() if isinstance(value, dict) else value if isinstance(value, list) else []
    for child in children:
        yield from walk(child)


def old_shape(node):
    if isinstance(node, list):
        return [old_shape(v) for v in node]
    if isinstance(node, dict):
        if node.get("kind") == "array":
            return {"kind": "unresolved", "reason": "array-length"}
        return {k: old_shape(v) for k, v in node.items()}
    return node


def expand_aliases(node, index, visiting=frozenset()):
    if isinstance(node, list):
        return [expand_aliases(n, index, visiting) for n in node]
    if not isinstance(node, dict):
        return node
    path = node.get("resolved_path")
    if path is not None:
        target = index.get(str(path["id"]), {}).get("inner", {}).get("type_alias")
        if target is not None and not target["generics"]["params"] and path["args"] is None:
            if path["id"] in visiting:
                raise ValueError("recursive rustdoc alias")
            return expand_aliases(target["type"], index, visiting | {path["id"]})
    return {k: expand_aliases(v, index, visiting) for k,v in node.items()}


def doc_fields(doc):
    fields = {}
    index = doc["index"]
    for item_id, row in doc["paths"].items():
        if row["crate_id"] != 0 or row["kind"] not in ("struct", "enum", "union"):
            continue
        item = index[item_id]["inner"][row["kind"]]
        path = tuple(row["path"])
        layouts = []
        if row["kind"] == "enum":
            for v in item["variants"]:
                variant = index[str(v)]
                layouts.append((path + (variant["name"],), variant["inner"]["variant"]["kind"]))
        elif row["kind"] == "union":
            layouts.append((path, {"plain": item}))
        else:
            layouts.append((path, item["kind"]))
        for prefix, layout in layouts:
            if not isinstance(layout, dict):
                continue
            for pos, field_id in enumerate(layout.get("tuple", layout.get("struct", layout.get("plain", {})).get("fields", []))):
                if field_id is None:
                    continue
                field = index[str(field_id)]
                key = (*prefix, field["name"] or str(pos))
                if key in fields:
                    raise ValueError(f"duplicate rustdoc field path {key}")
                fields[key] = expand_aliases(field["inner"]["struct_field"], index)
    return fields


def compare(previous, current, doc):
    if previous["vocabulary"] != "oddlang-data-shapes-v1" or current["vocabulary"] != "oddlang-data-shapes-v2":
        raise ValueError("unexpected shape versions")
    if old_shape(current["declarations"]) != previous["declarations"]:
        raise ValueError("array refinement changed other structural facts")
    gaps = {row["owner"] for row in previous["gaps"] if row["reason"] == "array-length"}
    if [g for g in previous["gaps"] if g["reason"] != "array-length"] != current["gaps"]:
        raise ValueError("unexpected change in remaining gaps")
    documentation = doc_fields(doc)
    observations = {}
    visible = set()
    for decl in current["declarations"]:
        path = tuple(p for p in decl["id"].removeprefix("local::").split("::") if p != "{type}")
        for row in [decl, *decl.get("variants", [])]:
            parent = path if row is decl else (*path, row["name"])
            for field in row.get("fields", []):
                key = (*parent, field["name"] or str(field["position"]))
                arrays = [n for n in walk(field["type"]) if isinstance(n, dict) and n.get("kind") == "array"]
                if key in documentation:
                    seen = [n["array"] for n in walk(documentation[key]) if isinstance(n, dict) and "array" in n]
                    visible.add(key)
                    actual = [(a["element"], a["extent"]) for a in arrays]
                    expected = [({"kind": "unsigned-integer", "bits": 8}, {"kind": "fixed", "count": int(v["len"])}) for v in seen if v["type"] == {"primitive":"u8"}]
                    if len(expected) != len(seen) or actual != expected:
                        raise ValueError(f"array element or extent mismatch at {key}: {actual}, {seen}")
                if arrays and key not in documentation and "{impl}" not in decl["id"]:
                    raise ValueError(f"missing independent rustdoc field: {key}")
                if arrays:
                    observations[field["id"]] = {"extents": [a["extent"] for a in arrays],
                                                 "rustdocFieldMatched": key in documentation}
    if set(observations) != gaps or not gaps:
        raise ValueError("missing original array gap or unexpected field")
    missing = [k for k,t in documentation.items() if k not in visible and any(isinstance(n, dict) and "array" in n for n in walk(t))]
    if missing:
        raise ValueError(f"rustdoc array missing from compiler graph: {missing}")
    return {"previousArrayGapOwners":len(gaps), "resolvedFields":observations,
            "rustdocCorroboratedFields":sum(v["rustdocFieldMatched"] for v in observations.values()),
            "declarations":len(current["declarations"]), "remainingGaps":dict(sorted(Counter(g["reason"] for g in current["gaps"]).items()))}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("previous", "current", "rustdoc", "receipt"):
        parser.add_argument(key, type=Path)
    args = parser.parse_args()
    report = compare(*(json.loads(p.read_bytes()) for p in (args.previous, args.current, args.rustdoc)))
    report["graphSha256"] = hashlib.sha256(args.current.read_bytes()).hexdigest()
    args.receipt.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
