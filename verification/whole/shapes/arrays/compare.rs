//! Cross-check the array refinement against v1 and a separate rustdoc projection.
use public_data_qualification::{Result, list, require, text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Path = Vec<String>;
type Fields = BTreeMap<Path, Vec<Value>>;

fn children(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array()
        .into_iter()
        .flatten()
        .chain(v.as_object().into_iter().flat_map(|o| o.values()))
}

fn previous_shape(value: &mut Value) {
    match value {
        Value::Object(fields) if fields.get("kind") == Some(&json!("array")) => {
            *value = json!({"kind":"unresolved","reason":"array-length"});
        }
        Value::Object(fields) => fields.values_mut().for_each(previous_shape),
        Value::Array(items) => items.iter_mut().for_each(previous_shape),
        _ => {}
    }
}

fn compiler_arrays<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    if value["kind"] == "array" {
        out.push(value);
    }
    for child in children(value) {
        compiler_arrays(child, out);
    }
}

fn expand_aliases(node: &mut Value, index: &Value, visiting: &mut BTreeSet<u64>) -> Result<()> {
    if let Some(path) = node.get("resolved_path") {
        let id = path["id"].as_u64().ok_or("invalid rustdoc path id")?;
        if let Some(alias) = index[id.to_string()]["inner"].get("type_alias") {
            if list(&alias["generics"]["params"])?.is_empty() && path["args"].is_null() {
                require(visiting.insert(id), "recursive rustdoc alias")?;
                *node = alias["type"].clone();
                expand_aliases(node, index, visiting)?;
                visiting.remove(&id);
                return Ok(());
            }
        }
    }
    match node {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                expand_aliases(child, index, visiting)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                expand_aliases(child, index, visiting)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn rustdoc_arrays(node: &Value, out: &mut Vec<Value>) {
    if let Some(array) = node.get("array") {
        out.push(array.clone());
    }
    for child in children(node) {
        rustdoc_arrays(child, out);
    }
}

fn collect_fields(
    fields: &mut Fields,
    index: &Value,
    path: &[String],
    layout: &Value,
) -> Result<()> {
    let ids = layout
        .get("tuple")
        .or_else(|| layout.get("struct").and_then(|s| s.get("fields")))
        .or_else(|| layout.get("plain").and_then(|s| s.get("fields")));
    let Some(ids) = ids else { return Ok(()) };
    for (position, id) in list(ids)?.iter().enumerate() {
        if id.is_null() {
            continue;
        }
        let field = &index[id.to_string()];
        let mut key = path.to_vec();
        key.push(
            field["name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| position.to_string()),
        );
        let mut arrays = Vec::new();
        let mut ty = field["inner"]
            .get("struct_field")
            .ok_or("missing rustdoc field")?
            .clone();
        expand_aliases(&mut ty, index, &mut BTreeSet::new())?;
        rustdoc_arrays(&ty, &mut arrays);
        require(
            fields.insert(key, arrays).is_none(),
            "duplicate rustdoc field path",
        )?;
    }
    Ok(())
}

fn document_fields(doc: &Value) -> Result<Fields> {
    let index = &doc["index"];
    let mut fields = Fields::new();
    for (id, row) in doc["paths"].as_object().ok_or("missing rustdoc paths")? {
        if row["crate_id"] != 0 {
            continue;
        }
        let kind = text(&row["kind"])?;
        if !matches!(kind, "enum" | "struct" | "union") {
            continue;
        }
        let item = index[id]["inner"].get(kind).ok_or("missing rustdoc item")?;
        let path: Path = serde_json::from_value(row["path"].clone())?;
        match kind {
            "enum" => {
                for id in list(&item["variants"])? {
                    let variant = &index[id.to_string()];
                    let mut prefix = path.clone();
                    prefix.push(text(&variant["name"])?.into());
                    collect_fields(
                        &mut fields,
                        index,
                        &prefix,
                        &variant["inner"]["variant"]["kind"],
                    )?;
                }
            }
            "union" => collect_fields(&mut fields, index, &path, &json!({"plain":item}))?,
            _ => collect_fields(&mut fields, index, &path, &item["kind"])?,
        }
    }
    Ok(fields)
}

pub fn compare(previous: &Value, current: &Value, doc: &Value) -> Result<Value> {
    require(
        previous["vocabulary"] == "oddlang-data-shapes-v1"
            && current["vocabulary"] == "oddlang-data-shapes-v2",
        "unexpected shape versions",
    )?;
    let mut old = current["declarations"].clone();
    previous_shape(&mut old);
    require(
        old == previous["declarations"],
        "array refinement changed other structural facts",
    )?;
    let mut owners = BTreeSet::new();
    let mut retained = Vec::new();
    for gap in list(&previous["gaps"])? {
        if gap["reason"] == "array-length" {
            owners.insert(text(&gap["owner"])?);
        } else {
            retained.push(gap);
        }
    }
    require(
        json!(retained) == current["gaps"],
        "unexpected change in remaining gaps",
    )?;
    let mut docs = document_fields(doc)?;
    let mut resolved = BTreeMap::new();
    let mut matched = 0;
    for decl in list(&current["declarations"])? {
        let id = text(&decl["id"])?;
        let path: Path = id
            .strip_prefix("local::")
            .unwrap_or(id)
            .split("::")
            .filter(|p| *p != "{type}")
            .map(str::to_owned)
            .collect();
        let variants = decl.get("variants").map(list).transpose()?;
        for (is_variant, row) in
            std::iter::once((false, decl)).chain(variants.into_iter().flatten().map(|v| (true, v)))
        {
            let mut parent = path.clone();
            if is_variant {
                parent.push(text(&row["name"])?.into());
            }
            let Some(fields) = row.get("fields") else {
                continue;
            };
            for field in list(fields)? {
                let mut key = parent.clone();
                key.push(
                    field["name"]
                        .as_str()
                        .filter(|n| !n.is_empty())
                        .map(str::to_owned)
                        .unwrap_or_else(|| field["position"].to_string()),
                );
                let mut arrays = Vec::new();
                compiler_arrays(&field["type"], &mut arrays);
                let observed = docs.remove(&key);
                if let Some(seen) = &observed {
                    require(
                        seen.len() == arrays.len(),
                        format!("array element or extent mismatch at {key:?}"),
                    )?;
                    for (s, a) in seen.iter().zip(&arrays) {
                        let count = text(&s["len"])?.parse::<u64>()?;
                        require(
                            s["type"] == json!({"primitive":"u8"})
                                && a["element"] == json!({"kind":"unsigned-integer","bits":8})
                                && a["extent"] == json!({"kind":"fixed","count":count}),
                            format!("array element or extent mismatch at {key:?}"),
                        )?;
                    }
                }
                if arrays.is_empty() {
                    continue;
                }
                require(
                    observed.is_some() || id.contains("{impl}"),
                    format!("missing independent rustdoc field {key:?}"),
                )?;
                matched += usize::from(observed.is_some());
                let extents: Vec<_> = arrays.iter().map(|a| &a["extent"]).collect();
                require(
                    resolved
                        .insert(
                            text(&field["id"])?,
                            json!({"extents":extents,"rustdocFieldMatched":observed.is_some()}),
                        )
                        .is_none(),
                    "duplicate compiler array field",
                )?;
            }
        }
    }
    require(
        !owners.is_empty() && owners.iter().eq(resolved.keys()),
        "missing original array gap or unexpected field",
    )?;
    require(
        docs.values().all(Vec::is_empty),
        "rustdoc array missing from compiler graph",
    )?;
    let mut reasons = BTreeMap::new();
    for gap in list(&current["gaps"])? {
        *reasons.entry(text(&gap["reason"])?).or_insert(0usize) += 1;
    }
    Ok(
        json!({"previousArrayGapOwners":owners.len(),"resolvedFields":resolved,"rustdocCorroboratedFields":matched,
        "declarations":list(&current["declarations"])?.len(),"remainingGaps":reasons}),
    )
}
