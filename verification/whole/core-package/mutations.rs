//! Prepare a single discriminating mutation in a verified, private copy of the inputs.
use clap::ValueEnum;
use public_data_qualification::{Result, json_bytes, load, require, sha, text};
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Clone, Copy, ValueEnum)]
pub enum Mutation {
    RetentionBound,
    RelayRuleOrder,
    FrameVersion,
    FrameErrorOrder,
    StaleFrameDigest,
}

fn write_sorted(path: &Path, mut value: Value) -> Result<()> {
    value.sort_all_objects();
    fs::write(path, json_bytes(&value)?)?;
    Ok(())
}

fn replace_once(path: &Path, before: &str, after: &str) -> Result<()> {
    let source = fs::read_to_string(path)?;
    require(
        source.matches(before).count() == 1,
        "mutation requires exactly one matching rule",
    )?;
    fs::write(path, source.replacen(before, after, 1))?;
    Ok(())
}

fn swap_first_pair(path: &Path, field: &str) -> Result<()> {
    let mut document = load(path)?;
    let items = document[field]
        .as_array_mut()
        .ok_or("expected rule array")?;
    require(
        items.len() >= 2 && items[0] != items[1],
        "expected two distinct rules",
    )?;
    items.swap(0, 1);
    write_sorted(path, document)
}

pub fn apply(root: &Path, mutation: Mutation) -> Result<()> {
    let manifest = root.join("core-package.json");
    let mut spec = load(&manifest)?;
    let (index, field) = match mutation {
        Mutation::RetentionBound => (0, "graph"),
        Mutation::RelayRuleOrder => (1, "graph"),
        Mutation::FrameVersion | Mutation::StaleFrameDigest => (2, "graph"),
        Mutation::FrameErrorOrder => (2, "overlay"),
    };
    let pin = spec["components"]
        .get_mut(index)
        .and_then(|c| c.get_mut(field))
        .ok_or("missing mutation input")?;
    let path = root.join(text(&pin["path"])?);
    match mutation {
        Mutation::RetentionBound => {
            replace_once(&path, "c:maximum 31536000", "c:maximum 31535999")?
        }
        Mutation::FrameVersion | Mutation::StaleFrameDigest => {
            replace_once(&path, "c:bits 32 ; c:value 1", "c:bits 32 ; c:value 2")?
        }
        Mutation::RelayRuleOrder => swap_first_pair(&path, "rules")?,
        Mutation::FrameErrorOrder => swap_first_pair(&path, "variantOrder")?,
    }
    if matches!(mutation, Mutation::StaleFrameDigest) {
        return Ok(());
    }
    pin["sha256"] = Value::String(sha(&path)?);
    write_sorted(&manifest, spec)
}
