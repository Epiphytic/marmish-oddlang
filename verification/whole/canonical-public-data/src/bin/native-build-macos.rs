use clap::Parser;
use public_data_qualification::{Result, macos_build};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    original: PathBuf,
    output: PathBuf,
    framework_pin: String,
    project: PathBuf,
    transcript: PathBuf,
    oracle: Option<PathBuf>,
}

fn main() -> Result<()> {
    let a = Args::parse();
    macos_build::generate(
        &a.framework,
        &a.original,
        &a.output,
        &a.framework_pin,
        &a.project,
        &a.transcript,
        a.oracle.as_deref(),
    )
}
