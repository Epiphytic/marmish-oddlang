//! Run the original-daemon and controlled-reply status oracles on installed binaries.
use clap::Parser;
use public_data_qualification::{
    Result,
    status_qualification::{Profile, qualify},
};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    profile: Profile,
    original_bins: PathBuf,
    generated_bins: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
}
fn main() -> Result<()> {
    let a = Args::parse();
    qualify(
        a.profile,
        &a.original_bins,
        &a.generated_bins,
        &a.receipt,
        &a.framework_revision,
    )
}
