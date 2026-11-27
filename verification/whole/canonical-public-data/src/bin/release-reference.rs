//! Compare the selected source tree with the independently measured release.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::bytes_sha, json_bytes, list, require, run, text,
};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

const SELECTED: &str = "af54339d250290490d62d4475a77539b7e739071";
const MEASURED: &str = "b666ae6aaa6063b1b30317d43fdb86bca3e4ec95";

#[derive(Parser)]
struct Args {
    /// Marmish Git checkout containing both pinned commits.
    checkout: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let git = |argv: &[&str]| run(Command::new("git").current_dir(&args.checkout).args(argv));
    let mut shared = json!({});
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "crates",
        "patches",
        "rust-toolchain.toml",
    ] {
        let object = |revision| git(&["rev-parse", "--verify", &format!("{revision}:{path}")]);
        let selected = object(SELECTED)?;
        require(
            object(MEASURED)? == selected,
            format!("release build input changed: {path}"),
        )?;
        shared[path] = json!(std::str::from_utf8(&selected)?.trim_end());
    }
    let raw = git(&[
        "show",
        &format!("{SELECTED}:docs/validation/v0.3.1-measurements.json"),
    ])?;
    let report: Value = serde_json::from_slice(&raw)?;
    require(
        report["source"] == MEASURED,
        "measurement provenance mismatch",
    )?;
    let mut cases = json!({});
    for cohort in list(&report["transfer_cohorts"])? {
        cases[text(&cohort["cohort"])?] = json!(
            cohort["passing_cases"]
                .as_u64()
                .ok_or("missing passing case count")?
        );
    }
    let paths = git(&["diff", "--name-only", MEASURED, SELECTED])?;
    let mut receipt = json!({
        "selectedCommit": SELECTED, "measuredCommit": MEASURED, "sharedBuildInputs": shared,
        "changedPaths": std::str::from_utf8(&paths)?.lines().collect::<Vec<_>>(),
        "measurementSha256": bytes_sha(&raw), "transferCases": cases,
        "candidateRun": report["candidate_run"].as_u64().ok_or("missing candidate run")?,
        "qualificationRun": report["qualification_run"].as_u64().ok_or("missing qualification run")?,
    });
    receipt.sort_all_objects();
    io::stdout().lock().write_all(&json_bytes(&receipt)?)?;
    Ok(())
}
