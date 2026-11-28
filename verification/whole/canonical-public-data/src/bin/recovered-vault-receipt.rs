//! Complete the recovered vault workspace receipt from the sealed build and native tests.
use clap::Parser;
use public_data_qualification::{
    Result, load, portable, recovered_vault, require, sha, text, whole,
};
use serde_json::{Value, json};
use std::{fs, io, path::PathBuf};

#[derive(Parser)]
struct Args {
    staged_manifest: PathBuf,
    generated_lock: PathBuf,
}

fn main() -> Result {
    let args = Args::parse();
    let manifest = args.staged_manifest.canonicalize()?;
    let inputs = manifest.parent().ok_or("missing staged input directory")?;
    let workspace = load(&manifest)?;
    let lock = portable().join("workspace-v7-recovered-vault-engine/workspace.lock");
    require(
        fs::read(&args.generated_lock)? == fs::read(&lock)?,
        "recovered workspace lock differs",
    )?;
    let selected = load(&lock)?;
    let before = load(&portable().join("workspace-v7-vault-engine/workspace.lock"))?;
    let unchanged = ["marmish-core", "marmish-engine", "marmish-stream"];
    for member in unchanged {
        let binding = selected["members"]
            .get(member)
            .ok_or("missing recovered member")?;
        require(
            binding.is_object() && before["members"].get(member) == Some(binding),
            format!("recovered workspace changed {member}"),
        )?;
    }
    #[derive(serde::Deserialize)]
    struct Observation {
        receipt: Value,
        fixed: Value,
    }
    let Observation {
        mut receipt,
        mut fixed,
    } = serde_json::from_reader(io::stdin().lock())?;
    require(
        receipt["workspaceLockSha256"] == sha(&lock)?
            && receipt["manifestSha256"] == sha(&manifest)?,
        "base receipt differs from recovered workspace inputs",
    )?;
    let verifier = whole().join("unified-vault/verify.py");
    require(
        receipt["verifierSha256"] == sha(&verifier)?,
        "base verifier digest differs",
    )?;
    fixed["standaloneReceiptSha256"] = recovered_vault::fixed(
        inputs,
        &workspace,
        text(&fixed["unchangedModuleSha256"])?,
        None,
    )?
    .into();
    receipt["baseVerifierSha256"] = receipt["verifierSha256"].clone();
    receipt["verifierSha256"] =
        sha(&whole().join("canonical-public-data/src/bin/recovered-vault-receipt.rs"))?.into();
    receipt["unchangedMemberLocks"] = json!(unchanged);
    receipt["recoveredBuildReport"] = recovered_vault::report(inputs, &workspace)?;
    receipt["fixedVaultValue"] = fixed;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
