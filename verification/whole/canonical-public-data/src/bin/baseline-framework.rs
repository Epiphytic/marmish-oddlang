//! Build the baseline compiler in a fresh target directory or verify its checkout.
#[path = "../../../linux/tagged_receipt.rs"]
mod tagged_receipt;
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, baseline_framework};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    VerifyTagged { framework: PathBuf },
    Verify { framework: PathBuf },
    Build { framework: PathBuf, output: PathBuf },
}
fn main() -> Result<()> {
    match Args::parse().command {
        Action::VerifyTagged { framework } => {
            println!("{}", tagged_receipt::verify(&framework)?);
            return Ok(());
        }
        Action::Verify { framework } => baseline_framework::verify(&framework)?,
        Action::Build { framework, output } => baseline_framework::build(&framework, &output)?,
    }
    println!("{}", baseline_framework::revision());
    Ok(())
}
