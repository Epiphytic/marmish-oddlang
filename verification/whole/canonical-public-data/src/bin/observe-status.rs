//! Observe one original foreground daemon and a bounded corpus of local status replies.
use clap::Parser;
use public_data_qualification::{Result, json_bytes, status_reference, whole};
use std::{fs, path::PathBuf};
#[derive(Parser)]
struct Args {
    original_bin_dir: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    candidate: Option<PathBuf>,
    #[arg(long)]
    build_receipt: Option<PathBuf>,
}
fn main() -> Result {
    let a = Args::parse();
    let build = a
        .build_receipt
        .unwrap_or_else(|| whole().join("bridge/aarch64-apple-darwin.json"));
    let report = status_reference::observe(&a.original_bin_dir, a.candidate.as_deref(), &build)?;
    fs::write(a.receipt, json_bytes(&report)?)?;
    Ok(())
}
