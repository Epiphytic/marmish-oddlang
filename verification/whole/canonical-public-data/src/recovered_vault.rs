//! Check the source-recovery and fixed-value evidence used by the composed vault workspaces.
use crate::{assembly::ORIGINAL, *};
use serde_json::{Value, json};
use std::path::Path;

fn pinned_path(inputs: &Path, reference: &Value) -> Result<std::path::PathBuf> {
    let root = inputs.canonicalize()?;
    let path = root.join(text(&reference["path"])?).canonicalize()?;
    require(
        path.starts_with(&root) && sha(&path)? == reference["sha256"],
        "unexpected staged vault workspace input",
    )?;
    Ok(path)
}

pub fn report(inputs: &Path, workspace: &Value) -> Result<Value> {
    let member = load(&pinned_path(
        inputs,
        &workspace["members"]["marmish-native"]["manifest"],
    )?)?;
    let reports: Vec<_> = list(&member["components"])?
        .iter()
        .filter(|r| r["kind"] == "build-report")
        .collect();
    let [component] = reports.as_slice() else {
        return Err("expected one build-report component".into());
    };
    let receipt = whole().join("release-info-structure/darwin-arm64/receipt.json");
    let prior = load(&receipt)?;
    let lane = &prior["lanes"]["baseline"];
    let graph = sha(&pinned_path(inputs, &component["graph"])?)?;
    require(
        prior["representedRevision"] == ORIGINAL
            && lane["sourceRevision"] == ORIGINAL
            && lane["composedGraphSha256"] == graph
            && component["graph"]["sha256"] == graph,
        "recovered report differs from standalone qualification",
    )?;
    Ok(json!({"graphSha256":graph,"recoveryReceiptSha256":sha(&receipt)?}))
}

pub fn fixed(
    inputs: &Path,
    workspace: &Value,
    module_digest: &str,
    predecessor: Option<&Path>,
) -> Result<String> {
    let receipt = whole().join("vault-fixed-bytes/aarch64-apple-darwin.json");
    let prior = load(&receipt)?;
    let package = &workspace["members"]["marmish-vault"]["manifest"];
    let vault = load(&pinned_path(inputs, package)?)?;
    let values: Vec<_> = list(&vault["components"])?
        .iter()
        .filter(|c| {
            c["reexports"]
                .as_array()
                .is_some_and(|r| r.iter().any(|n| n == "Secret"))
        })
        .collect();
    let [value] = values.as_slice() else {
        return Err("expected a fixed vault component".into());
    };
    pinned_path(inputs, &value["graph"])?;
    let package_hash = if let Some(p) = predecessor {
        let mut baseline = load(p)?;
        let mut successor = vault.clone();
        let inherited = baseline
            .as_object_mut()
            .and_then(|v| v.shift_remove("components"))
            .ok_or("missing predecessor components")?;
        let components = successor
            .as_object_mut()
            .and_then(|v| v.shift_remove("components"))
            .ok_or("missing successor components")?;
        let inherited = list(&inherited)?;
        let components = list(&components)?;
        require(
            baseline == successor
                && components.len() > inherited.len()
                && components.starts_with(inherited),
            "vault predecessor differs beyond appended components",
        )?;
        sha(p)?
    } else {
        text(&package["sha256"])?.to_owned()
    };
    require(
        prior["original"] == ORIGINAL
            && prior["framework"] == "595fb7a55dc2016cfab70c5b131bf2f6acfe9983"
            && prior["unchangedModuleSha256"] == module_digest
            && prior["graphSha256"] == value["graph"]["sha256"]
            && prior["packageManifestSha256"] == package_hash,
        "fixed-value package differs from exact native precedent",
    )?;
    sha(&receipt)
}
