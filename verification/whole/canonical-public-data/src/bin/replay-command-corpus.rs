//! Run the fixed local reference executables and compare their bytes and private receipts.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::bytes_sha, capture, json_bytes, load, require, sha, whole,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    upstream_fixture: PathBuf,
    fixture_sha256: String,
    /// Replay the pinned Python executable used to produce the historical receipt.
    #[arg(long)]
    historical_short: bool,
}

struct Case {
    args: Vec<OsString>,
    nonce: String,
    stdin: Vec<u8>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit: i32,
    identity: Value,
}
fn short(name: &str, stdin: &[u8], stdout: &[u8], stderr: &[u8], exit: i32) -> Case {
    let nonce = bytes_sha(name.as_bytes());
    Case {
        args: vec![name.into(), nonce.clone().into()],
        nonce,
        stdin: stdin.into(),
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit,
        identity: json!({"case":name}),
    }
}
fn finite(round: usize) -> Case {
    let seed =
        Sha256::digest(format!("oddlang finite benchmark vector 0 round {round}").as_bytes());
    let nonce = format!("{seed:x}");
    let vector = |length: usize, multiplier: usize, label: usize| {
        (0..length)
            .map(|i| (i * multiplier + usize::from(seed[(i / 256) % 32]) + label) as u8)
            .collect()
    };
    Case {
        args: vec![
            nonce.clone().into(),
            "0".into(),
            "".into(),
            r#"quoted ' " \ $() ; * ?"#.into(),
            "line\n雪".into(),
            OsString::from_vec(b"e\xffd".to_vec()),
        ],
        nonce,
        stdin: vector(16384, 1, 0),
        stdout: vector(131072, 17, 23),
        stderr: vector(131072, 17, 149),
        exit: 7,
        identity: json!({"vector":0}),
    }
}
fn install(source: &Path, target: &Path, pin: &str) -> Result {
    require(
        sha(source)? == pin,
        format!("fixture digest mismatch for {}", source.display()),
    )?;
    fs::copy(source, target)?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o500))?;
    require(sha(target)? == pin, "copied fixture digest mismatch")
}
fn replay(root: &Path, exe: &Path, case: Case) -> Result<Value> {
    let mut cmd = Command::new(exe);
    cmd.args(&case.args)
        .current_dir(root)
        .env_clear()
        .env("HOME", root)
        .env("TMPDIR", root)
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C");
    // umask is async-signal-safe and applies only to the child.
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    let output = capture(&mut cmd, 330, Some(&case.stdin))?;
    require(
        output.status.code() == Some(case.exit),
        "fixture exit differs",
    )?;
    require(output.stdout == case.stdout, "fixture stdout differs")?;
    require(output.stderr == case.stderr, "fixture stderr differs")?;
    let mut argv = Vec::new();
    for arg in std::iter::once(exe.as_os_str()).chain(case.args.iter().map(|a| a.as_os_str())) {
        let bytes = arg.as_bytes();
        argv.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
        argv.extend_from_slice(bytes);
    }
    let mut expected = json!({"schema":1,"nonce":case.nonce,"argv_sha256":bytes_sha(&argv),"executable_sha256":sha(exe)?,
        "stdin_bytes":case.stdin.len(),"stdin_sha256":bytes_sha(&case.stdin),"stdout_bytes":case.stdout.len(),"stdout_sha256":bytes_sha(&case.stdout),
        "stderr_bytes":case.stderr.len(),"stderr_sha256":bytes_sha(&case.stderr),"exit":case.exit});
    expected
        .as_object_mut()
        .unwrap()
        .extend(case.identity.as_object().unwrap().clone());
    let receipt = load(
        &root
            .join("receipts")
            .join(format!("{}.receipt", case.nonce)),
    )?;
    require(receipt == expected, "private fixture receipt differs")?;
    let row = expected.as_object_mut().unwrap();
    row.remove("nonce");
    row.remove("argv_sha256");
    Ok(expected)
}
fn main() -> Result {
    let args = Args::parse();
    let suite = whole().join("basic-command-interop");
    let build = load(&suite.join("reference-build-macos-arm64.json"))?;
    require(
        build["fixture"]["sha256"] == args.fixture_sha256,
        "unexpected upstream fixture pin",
    )?;
    let tmp = tempfile::Builder::new()
        .prefix("od-corpus-")
        .tempdir_in("/tmp")?;
    let root = tmp.path().canonicalize()?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    fs::create_dir(root.join("receipts"))?;
    fs::set_permissions(root.join("receipts"), fs::Permissions::from_mode(0o700))?;
    let local = root.join("short");
    if args.historical_short {
        install(
            &suite.join("corpus.py"),
            &local,
            build["corpus_source_sha256"]
                .as_str()
                .ok_or("missing short fixture pin")?,
        )?;
    } else {
        let target = tempfile::tempdir()?;
        public_data_qualification::run(
            Command::new("cargo")
                .args([
                    "+1.97.1",
                    "build",
                    "--quiet",
                    "--offline",
                    "--locked",
                    "--manifest-path",
                ])
                .arg(whole().join("canonical-public-data/Cargo.toml"))
                .args(["--bin", "short-command-fixture", "--target-dir"])
                .arg(target.path()),
        )?;
        let fixture = target.path().join("debug/short-command-fixture");
        install(&fixture, &local, &sha(&fixture)?)?;
    }
    let benchmark = root.join("finite");
    install(&args.upstream_fixture, &benchmark, &args.fixture_sha256)?;
    let mut rows = Vec::new();
    for case in [
        short("success", b"hello", b"done\n", b"", 0),
        short(
            "separate-streams",
            b"\0\xfe\n",
            b"stdout\0\xff",
            b"stderr\0\xfe",
            0,
        ),
        short("empty", b"", b"", b"", 0),
        short("trailing-newlines", b"next\n", b"one\r\n\n", b"two", 0),
        short("nonzero", b"finish", b"partial\n", b"controlled error\n", 7),
    ] {
        rows.push(replay(&root, &local, case)?);
    }
    for round in 0..4 {
        let mut row = replay(&root, &benchmark, finite(round))?;
        row["repetition"] = json!(round);
        rows.push(row);
    }
    tmp.close()?;
    require(!root.try_exists()?, "fixture workspace cleanup failed")?;
    let mut report = json!({"schema":1,"cases":rows,"workspace_removed":true});
    report.sort_all_objects();
    std::io::stdout().write_all(&json_bytes(&report)?)?;
    Ok(())
}
