//! Compare the installed release-info surfaces against independently selected binaries.
use clap::Parser;
use public_data_qualification::{Result, json_bytes};
use std::{
    io::{self, Write},
    path::PathBuf,
};
#[path = "../../../unified-mdk/release.rs"]
mod release;
#[derive(Parser)]
struct Args {
    original: PathBuf,
    portable: PathBuf,
    upstream_bins: PathBuf,
    candidate_bins: PathBuf,
    host: String,
}
fn main() -> Result {
    let args = Args::parse();
    let report = release::observe(
        &args.original.canonicalize()?,
        &args.portable.canonicalize()?,
        &args.upstream_bins.canonicalize()?,
        &args.candidate_bins.canonicalize()?,
        &args.host,
    )?;
    io::stdout().lock().write_all(&json_bytes(&report)?)?;
    Ok(())
}
