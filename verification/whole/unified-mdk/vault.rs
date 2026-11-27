//! Independent native vault corpus and unchanged original strict/holdout tests.
use public_data_qualification::{
    assembly::*,
    vault_witness::{self as witness, link, reference},
    *,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command};
pub fn observe(
    original: &Path,
    target: &Path,
    host: &str,
    directories: (&Path, &Path),
    library: &Path,
    workspace: &Value,
    predecessor: Option<&Path>,
) -> Result<Value> {
    let (work, scratch) = directories;
    let inputs = work.join("inputs");
    let deps = work.join("product/target/release/deps");
    let caller = whole().join("vault-values/caller.rs");
    let binary = scratch.join("vault-public-values");
    link(&caller, library, &deps, &binary, false)?;
    let original_caller = scratch.join("vault-original");
    reference(
        &original.join("crates/marmish-vault"),
        &original.join("Cargo.lock"),
        &original_caller,
        "observe-vault-values",
        &caller,
        false,
    )?;
    run(Command::new("cargo")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .args(["+1.97.1", "build", "--release", "--offline", "--locked"])
        .current_dir(&original_caller))?;
    let old = stdout(&mut Command::new(
        original_caller.join("target/release/observe-vault-values"),
    ))?;
    let new = stdout(&mut Command::new(binary))?;
    let rows = witness::expected(128);
    require(
        new.lines().eq(old.lines()) && new.lines().eq(rows.iter().map(String::as_str)),
        "sealed vault public values differ",
    )?;
    witness::strict(original, target, host, work, library, &deps)?;
    let receipt = whole().join("vault-fixed-bytes/aarch64-apple-darwin.json");
    let prior = load(&receipt)?;
    let package = &workspace["members"]["marmish-vault"]["manifest"];
    let vault = load(&inputs.join(text(&package["path"])?))?;
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
            && prior["unchangedModuleSha256"] == witness::MODULE
            && prior["graphSha256"] == value["graph"]["sha256"]
            && prior["packageManifestSha256"] == package_hash,
        "fixed-value package differs from exact native precedent",
    )?;
    let holdout = witness::holdout(original, work, library, &deps)?;
    Ok(json!({
        "name": witness::STRICT,
        "unchangedModuleSha256": witness::MODULE,
        "originalAndGeneratedPassed": true,
        "standaloneReceiptSha256": sha(&receipt)?,
        "fromBytesHoldout":holdout,
        "librarySha256": sha(library)?,
        "publicValueRows": rows.len(),
    }))
}
