//! Independent native vault corpus and unchanged original strict/holdout tests.
use public_data_qualification::{
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
    let receipt_sha = recovered_vault::fixed(&inputs, workspace, witness::MODULE, predecessor)?;
    let holdout = witness::holdout(original, work, library, &deps)?;
    Ok(json!({
        "name": witness::STRICT,
        "unchangedModuleSha256": witness::MODULE,
        "originalAndGeneratedPassed": true,
        "standaloneReceiptSha256": receipt_sha,
        "fromBytesHoldout":holdout,
        "librarySha256": sha(library)?,
        "publicValueRows": rows.len(),
    }))
}
