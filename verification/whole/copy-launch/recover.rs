//! Reconcile lexical recovery with the pinned compiler inventory and native enum observations.
use public_data_qualification::{
    Result, assembly::ORIGINAL, json_bytes, load, portable, require, run, sha, text,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

const COPY: &str = "urn:marmish:copy:";
const LAUNCH: &str = "urn:marmish:copy:launch";

fn array(v: &Value) -> Result<&Vec<Value>> {
    v.as_array().ok_or_else(|| "expected array".into())
}

fn one<'a>(mut matches: impl Iterator<Item = &'a Value>, what: &str) -> Result<&'a Value> {
    let item = matches.next().ok_or_else(|| format!("missing {what}"))?;
    require(matches.next().is_none(), format!("ambiguous {what}"))?;
    Ok(item)
}

fn names(record: &Value) -> Result<Vec<&str>> {
    array(&record["fields"])?
        .iter()
        .map(|f| text(&f["name"]))
        .collect()
}

fn declarations(host: &str) -> Result<Value> {
    let base = portable().join("workspace-shapes-v1");
    let lock = load(&base.join(format!(
        "locks/{host}.marmish-stream.lib.marmish_stream.json"
    )))?;
    require(
        lock["pin"]["sourceRevision"] == ORIGINAL,
        "compiler inventory source revision differs",
    )?;
    let digest = text(&lock["shapesSha256"])?;
    let path = base.join("graphs").join(format!("{digest}.json"));
    require(sha(&path)? == digest, "compiler inventory digest differs")?;
    load(&path)
}

fn launch(graph: &mut Value, binding: &mut Value, decls: &[Value], version: u64) -> Result {
    let mut unresolved = graph
        .as_object_mut()
        .and_then(|g| g.shift_remove("unresolved"))
        .ok_or("missing recovery gaps")?;
    unresolved
        .as_array_mut()
        .ok_or("invalid recovery gaps")?
        .sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    require(
        unresolved
            == json!([
                "source item has unverified native derive Default",
                "unexpanded source item macro numbered_enum"
            ]),
        "unexpected recovery gaps",
    )?;
    let native = one(
        decls.iter().filter(|d| d["name"] == "Launch"),
        "native Launch",
    )?;
    let record = one(
        array(&graph["types"])?.iter().filter(|d| d["id"] == LAUNCH),
        "recovered Launch",
    )?;
    require(
        names(native)? == names(record)?,
        "native and recovered Launch field names or order differ",
    )?;
    let guards = graph["guards"].as_array_mut().ok_or("missing guards")?;
    let mut guard = |label: &str, field: &str, rule: Value, error: &str| {
        let mut predicate = json!({"field":format!("{LAUNCH}:field:{field}")});
        predicate
            .as_object_mut()
            .unwrap()
            .extend(rule.as_object().unwrap().clone());
        guards.push(json!({"id":format!("{LAUNCH}:guard:{label}"),"owner":LAUNCH,"require":predicate,"error":format!("{COPY}{error}")}));
    };
    guard(
        "version",
        "version",
        json!({"kind":"unsigned-range","minimum":version,"maximum":version}),
        "copy_version",
    );
    for field in ["source", "destination"] {
        guard(
            &format!("{field}-length"),
            field,
            json!({"kind":"count","minimum":1,"maximum":4096}),
            "copy_path",
        );
        guard(
            &format!("{field}-content"),
            field,
            json!({"kind":"excludes-byte","byte":0}),
            "copy_path",
        );
    }
    let mut errors = json!({});
    for name in ["copy_version", "copy_path"] {
        errors[format!("{COPY}{name}")] = json!({"variant":"Invalid","argument":name});
    }
    binding["nativeValidation"] =
        json!({(LAUNCH):{"method":"validate","typePath":"crate::Error","errors":errors}});
    Ok(())
}

fn enums(table: &Value, decls: &[Value]) -> Result<(Value, Value)> {
    let base = portable().join("stream-shared-v1");
    let shared = load(&base.join("graph.json"))?;
    let hints = load(&base.join("rust.json"))?;
    let error = one(array(&shared["errors"])?.iter(), "shared error")?;
    let error_id = text(&error["id"])?;
    let version = format!("{COPY}VERSION");
    let mut names = json!({(&version):"VERSION"});
    names[error_id] = hints["names"][error_id].clone();
    let variants = array(&error["variants"])?;
    for v in variants {
        let id = text(&v["id"])?;
        names[id] = hints["names"]
            .get(id)
            .ok_or("missing error variant name")?
            .clone();
    }
    let invalid = one(
        variants
            .iter()
            .filter(|v| v["id"].as_str().is_some_and(|id| names[id] == "Invalid")),
        "Invalid error variant",
    )?;
    let mut orders = json!({(error_id):hints["variantOrder"][error_id]});
    let mut enums = vec![];
    for (name, rows) in table["enums"].as_object().ok_or("missing observed enums")? {
        let id = format!("{COPY}{name}");
        names[&id] = json!(name);
        let mut bytes = BTreeSet::new();
        let mut tag = |v: &Value| -> Result {
            let n = v
                .as_u64()
                .and_then(|i| u8::try_from(i).ok())
                .ok_or("invalid observed byte")?;
            require(bytes.insert(n), "duplicate observed byte")
        };
        for row in array(&rows["reject"])? {
            let r = array(row)?;
            require(
                r.len() == 3
                    && r[1] == "Invalid(\"copy_enum\")"
                    && r[2] == "invalid stream input: copy_enum",
                "unexpected enum rejection",
            )?;
            tag(&r[0])?;
        }
        let mut variants = vec![];
        let mut tags = json!({});
        for row in array(&rows["accept"])? {
            tag(&row["tag"])?;
            let variant = format!("{id}:{}", text(&row["rust"])?);
            require(tags.get(&variant).is_none(), "duplicate native variant")?;
            tags[&variant] = row["tag"].clone();
            names[&variant] = row["rust"].clone();
            variants.push(json!({"id":variant,"wire":row["wire"]}));
        }
        require(bytes.len() == 256, "incomplete enum byte coverage")?;
        let native = one(
            decls.iter().filter(|d| {
                d["name"] == *name
                    && d["kind"] == "sum"
                    && d["id"]
                        .as_str()
                        .is_some_and(|s| s.contains("::copy_protocol::"))
            }),
            name,
        )?;
        let order = array(&native["variants"])?
            .iter()
            .map(|v| Ok(format!("{id}:{}", text(&v["name"])?)))
            .collect::<Result<Vec<_>>>()?;
        let ids = order.iter().collect::<BTreeSet<_>>();
        require(
            ids.len() == order.len() && ids == tags.as_object().unwrap().keys().collect(),
            "native declaration variants differ from observed tags",
        )?;
        orders[&id] = json!(order);
        enums.push(json!({"id":id,"variants":variants,"numbers":{"tags":tags,"unknown":{"id":invalid["id"],"argument":"copy_enum"}}}));
    }
    let graph = json!({"format":"oddlang-shared-declarations-v1","id":format!("{COPY}numeric-constants-and-enums"),"constants":[{"id":version,"value":table["version"]}],"stringEnums":enums,"errors":[error]});
    let binding = json!({"format":"oddlang-shared-rust-v1","names":names,"constantTypes":{(version):"u32"},"variantOrder":orders,"externalErrorTypes":{(error_id):"crate::Error"}});
    Ok((graph, binding))
}

pub fn derive(original: &Path, cli: &Path, table: &Value, host: &str, out: &Path) -> Result {
    fs::create_dir(out)?;
    let data = portable().join("copy-launch-v1");
    let candidate = out.join("candidate");
    run(Command::new(cli)
        .args(["recover", "wire-schema"])
        .arg(original.join("crates/marmish-stream/src/copy_protocol.rs"))
        .arg(data.join("selection.json"))
        .arg(&candidate))?;
    let shape = declarations(host)?;
    let decls = array(&shape["declarations"])?;
    let mut graph = load(&candidate.join("graph.json"))?;
    let mut binding = load(&candidate.join("rust.json"))?;
    launch(
        &mut graph,
        &mut binding,
        decls,
        table["version"].as_u64().ok_or("missing version")?,
    )?;
    let (enum_graph, enum_binding) = enums(table, decls)?;
    for (name, value) in [
        ("launch.json", graph),
        ("launch-rust.json", binding),
        ("enums.json", enum_graph),
        ("enums-rust.json", enum_binding),
    ] {
        fs::write(out.join(name), json_bytes(&value)?)?;
        require(
            fs::read(out.join(name))? == fs::read(data.join(name))?,
            format!("recovered {name} differs from release"),
        )?;
    }
    require(
        fs::read(candidate.join("observation.json"))? == fs::read(data.join("observation.json"))?,
        "source observation differs from release",
    )
}
