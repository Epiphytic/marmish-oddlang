//! Run the retained-room fixture against a fresh generated library.
use clap::Parser;
use public_data_qualification::{Result, capture, portable, require, sha};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    compiler: PathBuf,
    target: PathBuf,
}

fn checked(cmd: &mut Command, timeout: u64) -> Result<String> {
    let output = capture(cmd, timeout, None)?;
    require(
        output.status.success(),
        format!(
            "{cmd:?}: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?)
}

fn attach_fixture(src: &Path) -> Result {
    let mut candidates = Vec::new();
    for group in fs::read_dir(src)? {
        let group = group?;
        if !group.file_type()?.is_dir()
            || !group.file_name().to_string_lossy().starts_with("group_")
        {
            continue;
        }
        for item in fs::read_dir(group.path())? {
            let item = item?;
            let name = item.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("__projection_")
                && name.ends_with(".rs")
                && item.file_type()?.is_file()
                && fs::read_to_string(item.path())?.contains("fn persist_verified_submit(")
            {
                candidates.push(item.path());
            }
        }
    }
    let [projection] = candidates.as_slice() else {
        return Err("expected one local submit projection".into());
    };
    let mut file = fs::OpenOptions::new().append(true).open(projection)?;
    file.write_all(b"\n")?;
    file.write_all(include_bytes!(
        "../../../basic-command-interop/authorized_submit_offline.rs"
    ))?;
    Ok(())
}

fn main() -> Result {
    let args = Args::parse();
    let scratch = tempfile::tempdir()?;
    let generated = scratch.path().join("generated");
    checked(
        Command::new(args.compiler.canonicalize()?)
            .args(["build", "native-workspace", "rust"])
            .arg(portable().join("workspace-authorized-owner-submit.json"))
            .arg(&generated),
        60,
    )?;
    eprintln!(
        "generated workspace lock {}",
        sha(&generated.join("workspace.lock"))?
    );
    attach_fixture(&generated.join("marmish-native/src"))?;
    let log = checked(
        Command::new("cargo")
            .current_dir(&generated)
            .args([
                "+1.97.1",
                "test",
                "--lib",
                "--offline",
                "--locked",
                "--no-run",
                "--message-format=json-render-diagnostics",
                "-p",
                "marmish-native",
                "--target-dir",
            ])
            .arg(std::path::absolute(args.target)?)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0"),
        1200,
    )?;
    let mut executables = Vec::new();
    for line in log.lines() {
        let row: Value = serde_json::from_str(line)?;
        if let Some(path) = row["executable"]
            .as_str()
            .filter(|_| row["profile"]["test"] == true)
        {
            executables.push(path.to_owned());
        }
    }
    let [binary] = executables.as_slice() else {
        return Err("expected one compiled fixture".into());
    };
    let state = scratch.path().join("fixture-state");
    fs::DirBuilder::new().mode(0o700).create(&state)?;
    let sandbox = || {
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.args(["-p", "(version 1)(allow default)(deny network*)", binary])
            .current_dir(&state)
            .env("MARMISH_OFFLINE_SUBMIT_FIXTURE", &state);
        cmd
    };
    let catalog = checked(sandbox().arg("--list"), 30)?;
    let cases: Vec<_> = catalog
        .lines()
        .filter_map(|l| l.strip_suffix(": test"))
        .filter(|n| {
            n.ends_with("::authorized_submit_offline::local_room_selection_and_commit_receipt")
        })
        .collect();
    let [case] = cases.as_slice() else {
        return Err("expected one retained-room test".into());
    };
    let result = checked(sandbox().args(["--exact", case]), 30)?;
    require(
        result
            .lines()
            .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
        "fixture did not pass exactly once",
    )?;
    print!("{result}");
    Ok(())
}
