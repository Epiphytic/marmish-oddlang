//! Recover and project the pinned gap-reason package.
use clap::Parser;
use public_data_qualification::{Result, gap_reason::recover};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    compiler: PathBuf,
    source: PathBuf,
    selection: PathBuf,
    output: PathBuf,
}

fn main() -> Result {
    let a = Args::parse();
    println!(
        "{}",
        recover(&a.compiler, &a.source, &a.selection, &a.output)?
    );
    Ok(())
}
