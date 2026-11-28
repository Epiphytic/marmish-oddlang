//! Check the public authorization-mode enum with the unchanged downstream Rust caller.
use clap::Parser;
use public_data_qualification::{Result, vault_mode::observe};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    library: PathBuf,
    scratch: PathBuf,
    host_deps: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!(
        "{}",
        observe(&args.library, &args.scratch, &args.host_deps)?
    );
    Ok(())
}
