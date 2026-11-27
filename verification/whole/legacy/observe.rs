//! Observe the pinned optional CLI through the original local version probe.
use clap::Parser;
use public_data_qualification::{Result, capture, json_bytes, load, require, sha, stdout, text};
use serde_json::json;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const FIXTURE: &str = "37c248a11239e108afc9bd4ee62b8f60542012e3";
const LOCK: &str = "d8a5367e49427c79f354b0611902808de9b92abbe173eb82ed6864d01e488a71";
const EXPECTED: &[u8] =
    b"Unverified binary observation; no runtime provenance or live egress approval.\nwn 0.10.3\n";

#[derive(Parser)]
struct Args {
    wn_source: PathBuf,
    wn_target: PathBuf,
    wnd: PathBuf,
    probe: PathBuf,
    build_receipt: PathBuf,
}

// Keep the human-readable platform label used by the original macOS and GNU/Linux receipts.
fn host() -> Result<String> {
    let uname =
        |flag| -> Result<String> { Ok(stdout(Command::new("uname").arg(flag))?.trim().into()) };
    let machine = uname("-m")?;
    let cpu = uname("-p")?;
    let processor = if cpu != machine && cpu != "unknown" {
        format!("-{cpu}")
    } else {
        String::new()
    };
    if cfg!(target_os = "macos") {
        let version = stdout(Command::new("sw_vers").arg("-productVersion"))?;
        return Ok(format!(
            "macOS-{}-{machine}{processor}-{}bit-Mach-O",
            version.trim(),
            usize::BITS
        ));
    }
    let os = uname("-s")?;
    let kernel = uname("-r")?;
    let mut label = format!("{os}-{kernel}-{machine}{processor}");
    if cfg!(target_os = "linux") {
        label.push_str("-with");
        if cfg!(target_env = "gnu") {
            let libc = stdout(Command::new("getconf").arg("GNU_LIBC_VERSION"))?;
            label.push('-');
            label.push_str(&libc.split_whitespace().collect::<String>());
        }
    }
    Ok(label)
}

fn build_wn(source: &Path, target: &Path) -> Result<PathBuf> {
    let git = |args: &[&str]| stdout(Command::new("git").current_dir(source).args(args));
    require(
        git(&["rev-parse", "HEAD"])?.trim() == FIXTURE
            && git(&["status", "--porcelain"])?.is_empty()
            && sha(&source.join("Cargo.lock"))? == LOCK,
        "unexpected wn build source",
    )?;
    let target = std::path::absolute(target)?;
    let status = Command::new("cargo")
        .current_dir(source)
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--release",
            "--offline",
            "-p",
            "wn-cli",
            "--bin",
            "wn",
            "--target-dir",
        ])
        .arg(&target)
        .status()?;
    require(status.success(), "locked wn build failed")?;
    Ok(target.join("release/wn"))
}

fn main() -> Result {
    let args = Args::parse();
    let wn = build_wn(&args.wn_source, &args.wn_target)?;
    let mut manifest = args.wnd.as_os_str().to_owned();
    manifest.push(".manifest.json");
    let daemon = load(Path::new(&manifest))?;
    require(
        daemon["revision"] == FIXTURE
            && daemon["cargo_lock_sha256"] == LOCK
            && daemon["sha256"] == sha(&args.wnd)?,
        "wnd provenance mismatch",
    )?;
    let native = load(&args.build_receipt)?;
    let revision = text(&native["commit"])?;
    let probe = args.probe.canonicalize()?;
    let digest = sha(&probe)?;
    require(
        native["builds"]["mdk"]["sha256"]["examples/probe"] == digest,
        "original probe hash mismatch",
    )?;
    let work = tempfile::tempdir()?;
    let observed = capture(
        Command::new(&probe)
            .arg(&wn)
            .env_clear()
            .current_dir(work.path()),
        8,
        None,
    )?;
    require(
        observed.status.success() && observed.stdout == EXPECTED && observed.stderr.is_empty(),
        "unexpected version probe exit or output",
    )?;
    let mut receipt = json!({
        "upstreamRevision": revision, "host": host()?, "fixtureRevision": FIXTURE,
        "wnBinarySha256": sha(&wn)?, "probeSha256": digest, "wnLockSha256": LOCK,
        "versionStdout": std::str::from_utf8(&observed.stdout)?, "versionStderr": "", "wndBuild": daemon,
    });
    receipt.sort_all_objects();
    std::io::stdout().lock().write_all(&json_bytes(&receipt)?)?;
    Ok(())
}
