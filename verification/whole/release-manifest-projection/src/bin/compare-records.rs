//! Compare the release Manifest and Asset records in two crates against their binding.
use clap::Parser;
use public_data_qualification::{Result, load};
use release_manifest_projection::{compare_records, rustdoc};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    original_root: PathBuf,
    original_target: PathBuf,
    generated_root: PathBuf,
    generated_target: PathBuf,
    generated_source: String,
    graph: PathBuf,
    binding: PathBuf,
}
fn main() -> Result {
    let a = Args::parse();
    let generated = rustdoc(&a.generated_root, &a.generated_target)?;
    let original = rustdoc(&a.original_root, &a.original_target)?;
    let shape = compare_records(
        &original,
        &generated,
        &a.generated_source,
        &load(&a.graph)?,
        &load(&a.binding)?,
    )?;
    println!("{}", serde_json::to_string(&shape)?);
    Ok(())
}
