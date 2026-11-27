//! Validate a stored native suite receipt against its pinned named catalog.
use std::{env, path::PathBuf, process::ExitCode};
use suite_receipts::{Result, inspect, read};

fn main() -> Result<ExitCode> {
    let mut complete = false;
    let mut path = None;
    for arg in env::args_os().skip(1) {
        if arg == "--require-complete" {
            complete = true
        } else if path.replace(PathBuf::from(arg)).is_some() {
            return Err("expected one receipt path".into());
        }
    }
    let totals = inspect(&read(
        &path.ok_or("usage: check-coverage RECEIPT [--require-complete]")?,
    )?)?;
    // Keep the published single-line summary used by shell consumers.
    println!(
        "{{\"benchmarks\": {}, \"ignored\": {}, \"passed\": {}, \"unverified\": {}}}",
        totals["benchmarks"], totals["ignored"], totals["passed"], totals["unverified"]
    );
    Ok(
        if complete
            && totals["unverified"]
                .as_u64()
                .ok_or("invalid unverified count")?
                > 0
        {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        },
    )
}
