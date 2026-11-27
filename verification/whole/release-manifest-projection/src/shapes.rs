//! Normalize the compiler's private field, trait and method observations for the two records.
use public_data_qualification::{Result, list, require, text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn key(id: &Value) -> Result<String> {
    Ok(id.as_u64().ok_or("expected rustdoc item ID")?.to_string())
}
fn item<'a>(index: &'a Value, id: &Value) -> Result<&'a Value> {
    index
        .get(key(id)?)
        .ok_or_else(|| "unknown rustdoc item ID".into())
}
fn route(paths: &Value, id: &Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value(item(paths, id)?["path"].clone())?)
}

fn ty(t: &Value, index: &Value, paths: &Value, records: &BTreeMap<&str, &Value>) -> Result<Value> {
    if let Some(name) = t["primitive"].as_str() {
        if name == "bool" {
            return Ok(json!({"kind":"boolean"}));
        }
        if matches!(name, "u8" | "u16" | "u32" | "u64") {
            return Ok(json!({"kind":"unsigned","bits":name[1..].parse::<u8>()?}));
        }
    }
    if t["borrowed_ref"]
        == json!({"lifetime":"'static","is_mutable":false,"type":{"primitive":"str"}})
    {
        return Ok(json!({"kind":"static-text"}));
    }
    if let Some(a) = t.get("array") {
        let element = ty(&a["type"], index, paths, records)?;
        require(element["kind"] == "unsigned", "non-integer array element")?;
        let length = text(&a["len"])?.parse::<usize>()?;
        return Ok(if element["bits"] == 8 {
            json!({"kind":"bytes","length":length})
        } else {
            json!({"kind":"unsigned-array","bits":element["bits"],"length":length})
        });
    }
    if let Some(path) = t.get("resolved_path") {
        let full = route(paths, &path["id"])?;
        if full == ["alloc", "string", "String"] && path["args"].is_null() {
            return Ok(json!({"kind":"owned-text"}));
        }
        if full == ["alloc", "vec", "Vec"] {
            let args = &path["args"]["angle_bracketed"];
            let [arg] = list(&args["args"])?.as_slice() else {
                return Err("expected one vector argument".into());
            };
            let element = &arg["type"]["resolved_path"];
            require(
                args["constraints"] == json!([]) && element["args"].is_null(),
                "unexpected vector generic parameters",
            )?;
            let row = item(index, &element["id"])?;
            let name = text(&row["name"])?;
            require(
                records.get(name).is_some_and(|r| r["id"] == row["id"]),
                "unexpected vector record identity",
            )?;
            return Ok(json!({"kind":"record-vector","record":name}));
        }
    }
    Err(format!("unsupported compiler-observed field type {t}").into())
}

pub fn shape(doc: &Value, filename: &str) -> Result<Value> {
    require(doc["format_version"] == 57, "expected Rustdoc JSON 57")?;
    let index = &doc["index"];
    let paths = &doc["paths"];
    let mut records = BTreeMap::new();
    for r in index.as_object().ok_or("missing rustdoc index")?.values() {
        if r["crate_id"] != 0 || r["span"]["filename"] != filename {
            continue;
        }
        let Some(name @ ("Manifest" | "Asset")) = r["name"].as_str() else {
            continue;
        };
        require(
            r["visibility"] == "crate" && records.insert(name, r).is_none(),
            "expected one crate-visible native record",
        )?;
        let body = &r["inner"]["struct"];
        require(
            body["kind"]["plain"]["has_stripped_fields"] == false
                && body["generics"]
                    .as_object()
                    .ok_or("missing generics")?
                    .values()
                    .all(|p| p == &json!([])),
            "expected complete non-generic named fields",
        )?;
    }
    require(
        records.keys().copied().collect::<BTreeSet<_>>() == BTreeSet::from(["Asset", "Manifest"]),
        "missing selected record",
    )?;
    let mut out = json!({});
    for (name, r) in &records {
        let body = &r["inner"]["struct"];
        let mut fields = Vec::new();
        let mut derives = Vec::new();
        let mut methods = Vec::new();
        for id in list(&body["kind"]["plain"]["fields"])? {
            let f = item(index, id)?;
            require(f["visibility"] == "public", "unexpected field visibility")?;
            fields.push(json!({"name":text(&f["name"])?,"type":ty(&f["inner"]["struct_field"],index,paths,&records)?}));
        }
        for id in list(&body["impls"])? {
            let imp = item(index, id)?;
            let b = &imp["inner"]["impl"];
            if b["is_synthetic"] == true || !b["blanket_impl"].is_null() {
                continue;
            }
            if b["trait"].is_null() {
                for id in list(&b["items"])? {
                    let m = item(index, id)?;
                    methods.push((text(&m["name"])?, &m["visibility"]));
                }
            } else {
                require(
                    list(&imp["attrs"])?.contains(&json!("automatically_derived")),
                    "unexpected authored implementation",
                )?;
                let full = route(paths, &b["trait"]["id"])?;
                let name = match full
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["core", "clone", "Clone"] => "clone",
                    ["core", "fmt", "Debug"] => "debug",
                    ["core", "cmp", "Eq"] => "eq",
                    ["core", "cmp", "PartialEq"] => "partial-eq",
                    ["core", "marker", "StructuralPartialEq"] => continue,
                    _ => return Err(format!("unsupported derived trait {full:?}").into()),
                };
                derives.push(name);
            }
        }
        methods.sort_by_key(|m| m.0);
        derives.sort();
        let methods: Vec<_> = methods
            .into_iter()
            .map(|(name, vis)| json!({"name":name,"visibility":vis}))
            .collect();
        out[*name] = json!({"visibility":r["visibility"],"fields":fields,"derives":derives,"methods":methods});
    }
    Ok(out)
}
pub fn declared(graph: &Value, overlay: &Value) -> Result<Value> {
    let results = &graph["recordResult"];
    let records = &overlay["records"];
    let mut out = json!({});
    for (id, fields) in results["records"]
        .as_object()
        .ok_or("missing record declarations")?
    {
        let binding = &records[id];
        let mut native = Vec::new();
        for f in list(fields)? {
            let mut mapped = f["type"].clone();
            if mapped["kind"] == "record-vector" {
                mapped["record"] = json!(text(&records[text(&mapped["record"])?]["name"])?);
            }
            native.push(json!({"name":text(&binding["fields"][text(&f["id"])?])?,"type":mapped}));
        }
        let mut api = Vec::new();
        if results["entry"]["record"] == id.as_str() {
            api.push(text(&overlay["parser"])?);
            for fmt in overlay["formatters"]
                .as_object()
                .ok_or("missing formatters")?
                .values()
            {
                api.push(text(fmt)?);
            }
        }
        api.sort();
        let methods: Vec<_> = api
            .into_iter()
            .map(|name| json!({"name":name,"visibility":overlay["visibility"]}))
            .collect();
        let mut derives: Vec<String> = serde_json::from_value(binding["derives"].clone())?;
        derives.sort();
        out[text(&binding["name"])?] = json!({"visibility":overlay["visibility"],"fields":native,"derives":derives,"methods":methods});
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use public_data_qualification::{load, portable, whole};
    #[test]
    fn declared_shape_matches_both_native_archives() -> Result {
        let graph = load(&portable().join("release-manifest-v1/graph.json"))?;
        let mut overlay = load(&portable().join("release-manifest-v1/rust.json"))?;
        let expected = declared(&graph, &overlay)?;
        for host in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
            let archive = load(&whole().join(format!("release-manifest-projection/{host}.json")))?;
            assert_eq!(expected, archive["resultShapes"]["fieldsAndTraits"]);
        }
        overlay["visibility"] = json!("public");
        assert_ne!(declared(&graph, &overlay)?, expected);
        Ok(())
    }
    #[test]
    fn reject_unsupported_compiler_type_and_lookalike_vec() {
        let foreign = json!({"resolved_path":{"id":1,"args":null}});
        let paths = json!({"1":{"path":["other","vec","Vec"]}});
        assert!(ty(&foreign, &Value::Null, &paths, &BTreeMap::new()).is_err());
        assert!(
            ty(
                &json!({"primitive":"i64"}),
                &Value::Null,
                &Value::Null,
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}
