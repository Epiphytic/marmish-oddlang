use clap::Parser;
use public_data_qualification::Result;
use std::path::PathBuf;
#[path = "../../../release-info-structure/ingest.rs"]
mod ingest;

#[derive(Parser)]
struct Args {
    oddlang: PathBuf,
    original: PathBuf,
    output: PathBuf,
    selection: PathBuf,
}
fn main() -> Result {
    let a = Args::parse();
    ingest::recover(&a.oddlang, &a.original, &a.output, &a.selection)
}
