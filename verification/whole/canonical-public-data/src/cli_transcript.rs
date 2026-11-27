//! Observe both installed parsers and compare their complete ordered transcripts.
use crate::{Result, capture, require};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::Command,
};
pub const ROLES: [&str; 2] = ["marmish", "marmishd"];

fn observe(binary: &Path, args: &[&str], home: &Path) -> Result<Value> {
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(home)
        .env_clear()
        .env("HOME", home)
        .env("LANG", "C")
        .env("PATH", "/usr/bin:/bin");
    // umask is process-local and async-signal-safe in the forked child.
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    let result = capture(&mut cmd, 8, Some(b""))?;
    require(
        result.stdout.is_ascii() && result.stderr.is_ascii(),
        "expected ASCII CLI output",
    )?;
    let status = result
        .status
        .code()
        .or_else(|| result.status.signal().map(|s| -s))
        .ok_or("missing exit status")?;
    Ok(
        json!({"args":args,"exit":status,"stdout":String::from_utf8(result.stdout)?,"stderr":String::from_utf8(result.stderr)?}),
    )
}

fn cases(binary: &Path, home: &Path) -> Result<Vec<Value>> {
    let root = observe(binary, &["--help"], home)?;
    let help = root["stdout"].as_str().ok_or("missing parser output")?;
    let section = help
        .split_once("\nCommands:\n")
        .ok_or("missing Commands section")?
        .1
        .split("\n\n")
        .next()
        .unwrap_or_default();
    let pattern = regex::Regex::new(r"(?m)^  ([\w-]+)\s")?;
    let commands: Vec<_> = pattern
        .captures_iter(section)
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();
    require(
        root["exit"] == 0
            && commands.contains(&"help")
            && commands.iter().collect::<BTreeSet<_>>().len() == commands.len(),
        "cannot enumerate root help",
    )?;
    let mut rows = Vec::new();
    for name in commands.into_iter().chain([
        "command-evidence",
        "command-cleanup-status",
        "command-local-cancel",
        "command-prepare-cleanup",
    ]) {
        let row = observe(binary, &["help", name], home)?;
        require(
            row["exit"] == 0 && row["stdout"] != "" && row["stderr"] == "",
            format!("failed to describe {name}"),
        )?;
        rows.push(row);
    }
    rows.insert(0, root);
    for flag in ["--version", "--oddlang-unrecognized-option"] {
        rows.push(observe(binary, &[flag], home)?);
    }
    Ok(rows)
}

pub fn collect(binaries: &Path) -> Result<Value> {
    let binaries = binaries.canonicalize()?;
    let home = tempfile::tempdir()?;
    let mut observed = json!({});
    for role in ROLES {
        observed[role] = json!(cases(&binaries.join(role), home.path())?);
    }
    require(
        fs::read_dir(home.path())?.next().is_none(),
        "CLI parse-only run modified its home",
    )?;
    Ok(observed)
}

pub fn verify(observed: &Value, expected: &Value) -> Result {
    for role in ROLES {
        let rows = observed[role].as_array().ok_or("invalid observations")?;
        for row in expected[role].as_array().ok_or("invalid fixture")? {
            require(
                rows.contains(row),
                format!("changed response: {role} {}", row["args"]),
            )?;
        }
    }
    require(
        observed == expected,
        "unexpected additional CLI commands or changed ordering",
    )?;
    Ok(())
}
