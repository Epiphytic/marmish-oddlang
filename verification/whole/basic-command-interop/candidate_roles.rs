//! Compare first-party Cargo executables with the staged workspace's CLI bindings.
use public_data_qualification::{Result, assembly::bytes_sha, list, require, text};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn read_pin(pin: &Value, base: &Path, root: &Path) -> Result<(Value, PathBuf)> {
    let path = base.join(text(&pin["path"])?).canonicalize()?;
    require(
        path.starts_with(root),
        "binding input outside staged workspace",
    )?;
    let bytes = fs::read(&path)?;
    require(
        pin["sha256"] == bytes_sha(&bytes),
        "binding input digest differs",
    )?;
    Ok((serde_json::from_slice(&bytes)?, path))
}

fn operations(binding: &Value) -> Result<Value> {
    let handlers = binding
        .get("handlers")
        .map(|value| value.as_object().ok_or("invalid handlers"))
        .transpose()?;
    let mut bound = Vec::new();
    let mut unbound = Vec::new();
    for (op, iri) in binding["operations"]
        .as_object()
        .ok_or("invalid operations")?
    {
        let iri = text(iri)?;
        if handlers.is_some_and(|rows| rows.contains_key(iri)) {
            if !op.is_empty() {
                bound.push(op);
            }
        } else {
            unbound.push(op);
        }
    }
    bound.sort();
    unbound.sort();
    Ok(
        json!({"boundOperations": bound, "unboundOperations": unbound, "uncoveredExit": binding.get("uncovered_exit").ok_or("missing uncovered exit")?}),
    )
}

pub fn inspect(
    inputs: &Path,
    descriptor: impl std::io::Read,
    binaries: Vec<String>,
) -> Result<Value> {
    let root = inputs.canonicalize()?;
    let workspace: Value = serde_json::from_reader(descriptor)?;
    let mut declared = BTreeSet::new();
    let mut roles = BTreeMap::new();
    for member in workspace["members"]
        .as_object()
        .ok_or("invalid workspace members")?
        .values()
    {
        let (package, manifest) = read_pin(&member["manifest"], &root, &root)?;
        if let Some(binaries) = package.get("binaries") {
            for binary in list(binaries)? {
                require(
                    declared.insert(text(&binary["name"])?.to_owned()),
                    "duplicate executable in workspace",
                )?;
            }
        }
        if let Some(components) = package.get("components") {
            for component in list(components)? {
                if text(&component["kind"])? != "cli-surface" {
                    continue;
                }
                let (binding, _) = read_pin(
                    &component["overlay"],
                    manifest.parent().ok_or("missing manifest parent")?,
                    &root,
                )?;
                let name = text(&binding["executable"])?;
                require(
                    roles
                        .insert(name.to_owned(), operations(&binding)?)
                        .is_none(),
                    "duplicate CLI binding",
                )?;
            }
        }
    }
    require(
        declared == binaries.into_iter().collect() && roles.keys().all(|k| declared.contains(k)),
        "first-party executable inventory differs from workspace",
    )?;
    Ok(serde_json::to_value(roles)?)
}
