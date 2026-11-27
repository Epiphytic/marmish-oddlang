//! Compare the archived and seeded traces through two public consumers.
use clap::Parser;
use public_data_qualification::{Result, json_bytes};
use std::{fs, path::PathBuf};
#[derive(Parser)]
struct Args {
    original: PathBuf,
    generated: PathBuf,
    receipt: PathBuf,
}
fn main() -> Result {
    let a = Args::parse();
    let r = stream_wire_replay::window::compare(
        &a.original.canonicalize()?,
        &a.generated.canonicalize()?,
    )?;
    Ok(fs::write(a.receipt, json_bytes(&r)?)?)
}
