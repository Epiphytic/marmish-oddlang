//! Compare staged recovered workspace inputs with both native precedent receipts.
use clap::Parser;
use public_data_qualification::{Result, load, recovered_vault};
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    manifest: PathBuf,
    module_digest: String,
    #[arg(long)]
    predecessor: Option<PathBuf>,
}
fn main() -> Result {
    let args = Args::parse();
    let manifest = args.manifest.canonicalize()?;
    let inputs = manifest.parent().ok_or("missing staged input directory")?;
    let workspace = load(&manifest)?;
    let report = recovered_vault::report(inputs, &workspace)?;
    let fixed = recovered_vault::fixed(
        inputs,
        &workspace,
        &args.module_digest,
        args.predecessor.as_deref(),
    )?;
    println!(
        "{}",
        json!({"recoveredBuildReport":report,"standaloneReceiptSha256":fixed})
    );
    Ok(())
}
