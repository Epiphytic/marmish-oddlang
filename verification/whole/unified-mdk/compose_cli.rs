use clap::Parser;
use public_data_qualification::{Result, six_composition::compose};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    portable: PathBuf,
    #[arg(long)]
    check: bool,
}
fn main() -> Result {
    let args = Args::parse();
    println!("{}", compose(&args.portable, args.check)?.display());
    Ok(())
}
