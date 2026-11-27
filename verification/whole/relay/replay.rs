//! Replay the pinned reference relay's independent loopback tests.
use clap::Parser;
use public_data_qualification::{Result, json_bytes, require, sha, stdout};
use serde_json::json;
use std::{io::Write, path::PathBuf, process::Command};

const REVISION: &str = "b38b2841b081952dc7728da391fe0e26fc7c6181";
const CASES: [&str; 3] = [
    "challenges_first_and_finishes_accepted_publish_after_sender_disconnect",
    "challenge_precedes_a_pipelined_client_request",
    "cli_reports_actual_bind_and_structured_logs_redact_wire_data",
];

#[derive(Parser)]
struct Args {
    checkout: PathBuf,
    target_dir: PathBuf,
}

fn check_test(name: &str, output: &str) -> Result {
    let passed = format!("test {name} ... ok");
    require(
        output.lines().any(|line| line == passed)
            && output
                .lines()
                .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
        format!("missing exact passing test: {name}"),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let root = args.checkout.canonicalize()?;
    let target = std::path::absolute(args.target_dir)?;
    let git = |args: &[&str]| stdout(Command::new("git").current_dir(&root).args(args));
    require(
        git(&["rev-parse", "HEAD"])?.trim() == REVISION
            && git(&["status", "--porcelain"])?.is_empty(),
        "reference relay checkout must be clean and pinned",
    )?;
    for name in CASES {
        let output = stdout(
            Command::new("cargo")
                .current_dir(&root)
                .args([
                    "+1.97.1",
                    "test",
                    "--release",
                    "--locked",
                    "--offline",
                    "--target-dir",
                ])
                .arg(&target)
                .args([
                    "-p",
                    "deaddrop-server",
                    "--test",
                    "debug_ws",
                    "--",
                    name,
                    "--exact",
                    "--test-threads=1",
                ]),
        )?;
        check_test(name, &output)?;
    }
    let toolchain = stdout(
        Command::new("rustc")
            .current_dir(&root)
            .args(["+1.97.1", "-vV"]),
    )?;
    let report = json!({
        "commit": REVISION, "toolchain": toolchain.trim(), "lockSha256": sha(&root.join("Cargo.lock"))?,
        "profile": "release", "transport": "loopback websocket", "passed": CASES,
    });
    std::io::stdout().lock().write_all(&json_bytes(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_name_and_single_pass_are_required() {
        let output = "test probe ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;";
        assert!(check_test("probe", output).is_ok());
        assert!(check_test("other", output).is_err());
        assert!(check_test("probe", &output.replace("1 passed", "11 passed")).is_err());
        assert!(check_test("probe", &output.replace("0 failed", "1 failed")).is_err());
    }
}
