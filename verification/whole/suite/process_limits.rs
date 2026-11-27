//! Run the pinned upstream help test with explicitly staged client and daemon paths.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::ORIGINAL, capture, json_bytes, require, run, sha, stdout,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const SOURCE: &str = "crates/marmish-native/tests/process_limits.rs";
const CASE: &str = "actual_native_entrypoints_accept_inherited_limits";

#[derive(Parser)]
struct Args {
    upstream: PathBuf,
    #[arg(long)]
    rustc: PathBuf,
    #[arg(long)]
    libc: PathBuf,
    #[arg(long)]
    client: PathBuf,
    #[arg(long)]
    daemon: PathBuf,
    #[arg(long)]
    cargo_harness: Option<PathBuf>,
}

fn stage_env(cmd: &mut Command, work: &Path) {
    cmd.current_dir(work);
    for role in ["marmish", "marmishd"] {
        cmd.env(format!("CARGO_BIN_EXE_{role}"), work.join(role));
    }
}

fn expect_success(output: &Output) -> Result {
    require(
        output.status.success(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

fn execute(test: &Path, work: &Path, missing_overrides: bool) -> Result<Output> {
    let mut cmd = Command::new(test);
    stage_env(&mut cmd, work);
    if missing_overrides {
        for role in ["marmish", "marmishd"] {
            cmd.env(format!("CARGO_BIN_EXE_{role}"), work.join("missing"));
        }
    }
    let result = capture(cmd.args(["--exact", CASE]), 30, None)?;
    require(
        String::from_utf8_lossy(&result.stdout)
            .lines()
            .filter(|l| *l == "running 1 test")
            .count()
            == 1,
        "expected exactly one upstream test",
    )?;
    Ok(result)
}

fn qualify(args: Args) -> Result<Value> {
    let git = || {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&args.upstream);
        cmd
    };
    require(
        stdout(git().args(["rev-parse", "HEAD"]))?.trim() == ORIGINAL,
        format!("expected upstream HEAD {ORIGINAL}"),
    )?;
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    let source = root.join("process_limits.rs");
    fs::write(
        &source,
        run(git().args(["show", &format!("{ORIGINAL}:{SOURCE}")]))?,
    )?;
    let roles = [("marmish", &args.client), ("marmishd", &args.daemon)];
    let mut binaries = json!({});
    for (role, original) in roles {
        let path = root.join(role);
        fs::copy(original, &path)?;
        binaries[role] = json!(sha(&path)?);
    }
    let rustc = args.rustc.canonicalize()?;
    let libc = args.libc.canonicalize()?;
    let harness = root.join("process_limits");
    let mut compile = Command::new(&rustc);
    stage_env(&mut compile, &root);
    let mut dependency = std::ffi::OsString::from("libc=");
    dependency.push(&libc);
    compile
        .args(["--edition=2024", "--test"])
        .arg(&source)
        .arg("--extern")
        .arg(dependency)
        .arg("-o")
        .arg(&harness);
    expect_success(&capture(&mut compile, 120, None)?)?;
    let good = execute(&harness, &root, false)?;
    expect_success(&good)?;
    let mut receipt = json!({"upstream_commit":ORIGINAL,"source":SOURCE,"test":CASE,
        "source_sha256":sha(&source)?,"compiler":stdout(Command::new(&rustc).arg("--version"))?.trim(),
        "libc_sha256":sha(&libc)?,"executables":binaries,"staged_pair_exit":good.status.code(),"mutants":{}});
    for (role, original) in roles {
        let path = root.join(role);
        fs::remove_file(&path)?;
        fs::write(&path, b"#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        let bad = execute(&harness, &root, false)?;
        require(
            bad.status.code() == Some(101)
                && String::from_utf8_lossy(&bad.stdout)
                    .contains("assertion failed: !result.stdout.is_empty()"),
            format!("missing empty-stdout failure for {role}"),
        )?;
        receipt["mutants"][role] = json!({"sha256":sha(&path)?,"exit":bad.status.code(),"failed_assertion":"!result.stdout.is_empty()"});
        fs::remove_file(&path)?;
        fs::copy(original, &path)?;
    }
    if let Some(test) = args.cargo_harness {
        let result = execute(&test.canonicalize()?, &root, true)?;
        expect_success(&result)?;
        receipt["cargo_harness_with_missing_runtime_overrides"] =
            json!({"harness_sha256":sha(&test)?,"exit":result.status.code()});
    }
    Ok(receipt)
}

fn main() -> Result {
    io::stdout()
        .lock()
        .write_all(&json_bytes(&qualify(Args::parse())?)?)?;
    Ok(())
}
