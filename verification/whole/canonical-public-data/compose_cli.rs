use clap::Parser;
use public_data_qualification::{Result, composition::compose, portable};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    check: bool,
    #[arg(long,default_value_os_t=portable())]
    portable: PathBuf,
}
fn main() -> Result {
    let args = Args::parse();
    compose(args.portable, args.check)
}
