//! Capture and verify the parse-only Marmish CLI transcript.
use clap::{Parser, ValueEnum};
use public_data_qualification::{Result, capture, json_bytes, load, require, sha, whole};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Capture,
    Verify,
}
#[derive(Parser)]
struct Args {
    mode: Mode,
    bindir: PathBuf,
    #[arg(long,default_value_os_t=whole().join("cli/transcript.json"))]
    fixture: PathBuf,
    #[arg(long,default_value_os_t=whole().join("bridge/aarch64-apple-darwin.json"))]
    build_receipt: PathBuf,
}

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

fn main() -> Result<()> {
    let args = Args::parse();
    let roles = ["marmish", "marmishd"];
    let binaries = args.bindir.canonicalize()?;
    if matches!(args.mode, Mode::Capture) {
        let receipt = load(&args.build_receipt)?;
        for role in roles {
            require(
                receipt["builds"]["default"]["sha256"][role] == sha(&binaries.join(role))?,
                format!("unrecognized original binary: {role}"),
            )?;
        }
    }
    let home = tempfile::tempdir()?;
    let mut observed = json!({});
    for role in roles {
        observed[role] = json!(cases(&binaries.join(role), home.path())?);
    }
    require(
        fs::read_dir(home.path())?.next().is_none(),
        "CLI parse-only run modified its home",
    )?;
    match args.mode {
        Mode::Capture => {
            let mut stage = tempfile::NamedTempFile::new_in(
                args.fixture
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?;
            stage.write_all(&json_bytes(&observed)?)?;
            stage.persist_noclobber(args.fixture)?;
        }
        Mode::Verify => {
            let expected = load(&args.fixture)?;
            for role in roles {
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
        }
    }
    println!(
        "{{'marmish': {}, 'marmishd': {}}}",
        observed["marmish"].as_array().unwrap().len(),
        observed["marmishd"].as_array().unwrap().len()
    );
    Ok(())
}
