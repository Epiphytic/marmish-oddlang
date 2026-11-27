//! Compose the fieldless engine choices, authored range and source-recovered range releases.
use crate::{
    api_composition::{Descriptor, set_member},
    *,
};
use serde_json::{Value, json};
use std::path::Path;
const ENGINE: &str = "marmish-engine";
const ENGINE_CHOICES: &str = "engine-fieldless-choices-package.json";
const REASON: &str = "urn:marmish:engine:output-gap:reason";
fn read(root: &Path, name: &str) -> Result<Value> {
    load(&root.join(name))
}
fn checked(root: &Path, descriptor: &Value) -> Result<Value> {
    let name = text(&descriptor["path"])?;
    require(
        pin(root, name)? == *descriptor,
        format!("stale portable pin {name}"),
    )?;
    read(root, name)
}
fn role(component: &Value, kind: &str, exports: &[&str]) -> Result {
    require(
        component["kind"] == kind
            && component["module"] == "model"
            && component["reexports"] == json!(exports),
        format!("unexpected component for {}", exports.join(", ")),
    )
}
fn inputs(root: &Path, workspace: &str) -> Result<(Value, Value)> {
    let workspace = read(root, workspace)?;
    let engine = checked(root, &workspace["members"][ENGINE]["manifest"])?;
    Ok((workspace, engine))
}
pub(crate) fn choices(root: &Path) -> Result<Vec<Descriptor>> {
    let mut workspace = read(root, "workspace-v7-canonical-offline.json")?;
    let slot = &workspace["members"][ENGINE]["manifest"];
    let predecessor = text(&slot["path"])?;
    require(
        slot["sha256"] == sha(&root.join(predecessor))?,
        "canonical engine manifest changed",
    )?;
    let mut old = read(root, predecessor)?;
    let new = read(root, ENGINE_CHOICES)?;
    let appended = list(&new["components"])?
        .last()
        .ok_or("missing engine choice component")?;
    old.get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("missing canonical engine components")?
        .push(appended.clone());
    require(new == old, "engine composition changed inherited inputs")?;
    role(appended, "checked-values", &["Stream", "SubmitDisposition"])?;
    workspace["members"][ENGINE]["manifest"] = pin(root, ENGINE_CHOICES)?;
    Ok(vec![Descriptor::new(
        "workspace-v7-canonical-engine-choices.json",
        &workspace,
    )?])
}
fn output(
    mut workspace: Value,
    engine: Value,
    package: &'static str,
    release: &'static str,
) -> Result<Vec<Descriptor>> {
    let engine = Descriptor::new(package, &engine)?;
    set_member(&mut workspace, ENGINE, &engine)?;
    Ok(vec![engine, Descriptor::new(release, &workspace)?])
}
pub(crate) fn range(root: &Path) -> Result<Vec<Descriptor>> {
    let (workspace, mut engine) = inputs(root, "workspace-v7-canonical-public-data.json")?;
    let donor = read(root, "engine-gap-range-package.json")?;
    let [standalone, record] = list(&donor["components"])?.as_slice() else {
        return Err("expected only a reason and range component".into());
    };
    let parts = engine
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("missing engine components")?;
    let reason = parts.last().ok_or("missing qualified reason")?;
    role(reason, "shared-declarations", &["MissingReason"])?;
    role(record, "public-records", &["MissingRange"])?;
    for key in ["graph", "overlay"] {
        require(
            checked(root, &reason[key])? == checked(root, &standalone[key])?,
            format!("the qualified reason {key} differs from the standalone one"),
        )?;
    }
    let graph = checked(root, &record["graph"])?;
    let [wire] = list(&graph["modules"][0]["records"])?.as_slice() else {
        return Err("expected one range record".into());
    };
    let fields = list(&wire["fields"])?;
    let mut external = std::collections::BTreeSet::new();
    for field in fields {
        let ty = field.get("type").ok_or("range field has no type")?;
        if let Some(id) = ty.get("external") {
            external.insert(text(id)?);
        }
    }
    require(
        external == [REASON].into()
            && checked(root, &record["overlay"])?["externalTypes"]
                == json!({REASON:["crate","MissingReason"]}),
        "range record does not bind the existing MissingReason",
    )?;
    parts.push(record.clone());
    output(
        workspace,
        engine,
        "marmish-engine-canonical-range-data-package.json",
        "workspace-v7-canonical-range-data.json",
    )
}
pub(crate) fn recovered(root: &Path) -> Result<Vec<Descriptor>> {
    let (workspace, mut engine) = inputs(root, "workspace-v7-canonical-range-data.json")?;
    let donor = read(root, "engine-gap-range-recovered-package.json")?;
    let [reason, range] = list(&donor["components"])?.as_slice() else {
        return Err("expected recovered reason and range components".into());
    };
    for (component, folder, kind, name) in [
        (
            reason,
            "engine-gap-reason-recovered-v1",
            "shared-declarations",
            "MissingReason",
        ),
        (
            range,
            "engine-gap-range-recovered-v1",
            "public-records",
            "MissingRange",
        ),
    ] {
        role(component, kind, &[name])?;
        for (key, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
            require(
                component[key] == pin(root, &format!("{folder}/{file}"))?,
                format!("recovered {name} must use the published {file} bytes"),
            )?;
        }
    }
    let parts = engine
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("missing engine components")?;
    let [.., prior_reason, before] = parts.as_mut_slice() else {
        return Err("missing predecessor reason or range".into());
    };
    require(
        prior_reason == reason,
        "the already recovered reason changed",
    )?;
    let mut replacement = range.clone();
    for key in ["graph", "overlay"] {
        require(
            checked(root, &before[key])? == checked(root, &range[key])?
                && before[key]["sha256"] != range[key]["sha256"],
            format!("recovered range {key} must preserve the contract and change the raw digest"),
        )?;
        replacement[key] = before[key].clone();
    }
    require(
        replacement == *before,
        "recovered range changed descriptor fields",
    )?;
    *before = range.clone();
    output(
        workspace,
        engine,
        "marmish-engine-canonical-recovered-range-package.json",
        "workspace-v7-canonical-recovered-range.json",
    )
}
