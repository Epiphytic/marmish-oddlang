//! Replay the dev and release native stream observations against the frozen framework oracle.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, assembly::checkout, baseline_framework, json_bytes, load, require, run, sha, stdout,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const ORACLE_REVISION: &str = "08373b871aeac0f76cb1675748fd604cea2a79a2";
const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const LOCK: &str = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3";
const FIELDS: [&str; 7] = [
    "publicObservationSha256",
    "priorCases",
    "authoredCases",
    "seededCases",
    "mixedBinaryAndJson",
    "negativeControls",
    "freezeSha256",
];

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Compile the pinned Rust oracle, run both native builds and publish their receipts.
    Run {
        framework: PathBuf,
        original: PathBuf,
        out: PathBuf,
    },
    /// Check an already-collected pair against the frozen expectations and its declared build provenance.
    Verify {
        framework: PathBuf,
        target: String,
        reports: PathBuf,
    },
}

fn expected(framework: &Path) -> Result<Value> {
    baseline_framework::verify(framework)?;
    load(&framework.join("examples/marmish/portable-oracle/original-receipt.json"))
}
fn verify(report: &Value, baseline: &Value, profile: &str, target: &str) -> Result {
    for field in FIELDS {
        let value = baseline
            .get(field)
            .ok_or_else(|| format!("missing baseline field {field}"))?;
        require(
            report.get(field) == Some(value),
            format!("{profile}: mismatch in {field}"),
        )?;
    }
    let provenance = &report["provenance"];
    for (key, value) in [
        ("revision", ORIGINAL),
        ("cargoLockSha256", LOCK),
        ("target", target),
        ("profile", profile),
    ] {
        require(
            provenance[key] == value,
            format!("{profile}: mismatch in provenance.{key}"),
        )?;
    }
    Ok(())
}
fn check_pair(dir: &Path, baseline: &Value, target: &str) -> Result<Value> {
    require(
        matches!(
            target,
            "aarch64-apple-darwin" | "aarch64-unknown-linux-gnu" | "x86_64-unknown-linux-gnu"
        ),
        "unsupported native host",
    )?;
    let mut pins = json!({});
    for profile in ["dev", "release"] {
        let path = dir.join(format!("{profile}.json"));
        verify(&load(&path)?, baseline, profile, target)?;
        pins[profile] = json!(sha(&path)?);
    }
    Ok(pins)
}

fn build_oracle(framework: &Path, work: &Path) -> Result<PathBuf> {
    let archive = work.join("sources.tar");
    run(Command::new("git")
        .current_dir(framework)
        .args(["archive", "-o"])
        .arg(&archive)
        .args([
            ORACLE_REVISION,
            "verification/marmish-inventory",
            "verification/offline.rs",
            "examples/marmish",
        ]))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(work))?;
    fs::remove_file(archive)?;
    let target = work.join("build");
    let mut cmd = Command::new("cargo");
    for (key, _) in env::vars_os() {
        let k = key.to_string_lossy();
        if k.starts_with("RUST") && k != "RUSTUP_HOME"
            || k.starts_with("CARGO_BUILD_")
            || k.starts_with("CARGO_TARGET_")
            || k.starts_with("CARGO_ENCODED_")
            || k.starts_with("CARGO_PROFILE_")
        {
            cmd.env_remove(key);
        }
    }
    run(cmd
        .current_dir(work)
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "--manifest-path",
            "verification/marmish-inventory/Cargo.toml",
            "--bin",
            "portable-oracle",
            "--target-dir",
        ])
        .arg(&target))?;
    Ok(target.join("debug/portable-oracle"))
}

fn host_info() -> Result<(String, Value)> {
    let kernel = stdout(Command::new("uname").arg("-r"))?;
    if cfg!(target_os = "macos") {
        let version = stdout(Command::new("sw_vers").arg("-productVersion"))?;
        return Ok((
            kernel.trim().into(),
            json!({"os":format!("macOS {}",version.trim()),"libc":["",""]}),
        ));
    }
    let release = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))?;
    let name = release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .ok_or("missing Linux distribution name")?;
    let libc = stdout(Command::new("getconf").arg("GNU_LIBC_VERSION"))?;
    let (family, version) = libc.trim().split_once(' ').ok_or("missing libc version")?;
    Ok((
        kernel.trim().into(),
        json!({"os":name.trim_matches('"'),"libc":[family,version]}),
    ))
}
fn replay(framework: &Path, original: &Path, out: &Path) -> Result {
    let framework = framework.canonicalize()?;
    let original = original.canonicalize()?;
    let baseline = expected(&framework)?;
    checkout(&original, ORIGINAL, false)?;
    require(
        sha(&original.join("Cargo.lock"))? == LOCK,
        "original Cargo.lock differs",
    )?;
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let target = rustc
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing native host")?;
    require(
        matches!(
            (env::consts::OS, env::consts::ARCH, target),
            ("linux", "aarch64", "aarch64-unknown-linux-gnu")
                | ("linux", "x86_64", "x86_64-unknown-linux-gnu")
                | ("macos", "aarch64", "aarch64-apple-darwin")
        ),
        "native Linux or macOS ARM64 toolchain required",
    )?;
    let work = tempfile::tempdir()?;
    let oracle = build_oracle(&framework, work.path())?;
    fs::create_dir_all(out)?;
    let stage = tempfile::tempdir_in(out)?;
    for profile in ["dev", "release"] {
        run(Command::new(&oracle)
            .arg("--upstream")
            .arg(&original)
            .args([
                "--original-target",
                target,
                "--original-revision",
                ORIGINAL,
                "--original-lock",
                LOCK,
                "--original-profile",
                profile,
                "--receipt",
            ])
            .arg(stage.path().join(format!("{profile}.json"))))?;
    }
    let receipts = check_pair(stage.path(), &baseline, target)?;
    baseline_framework::verify(&framework)?;
    checkout(&original, ORIGINAL, false)?;
    let (kernel, mut host) = host_info()?;
    host["kernel"] = json!(kernel);
    host["frameworkRevision"] = json!(baseline_framework::revision());
    host["rustHost"] = json!(target);
    host["receipts"] = receipts;
    host["oracleRevision"] = json!(ORACLE_REVISION);
    host["oracleSha256"] = json!(sha(&oracle)?);
    host.sort_all_objects();
    fs::write(stage.path().join("host.json"), json_bytes(&host)?)?;
    publish(stage.path(), out)
}

fn publish(stage: &Path, out: &Path) -> Result {
    // Serialize publication for concurrent replay jobs. host.json binds the two profile digests.
    let directory = fs::File::open(out)?;
    directory.lock()?;
    let backups = tempfile::tempdir_in(out)?;
    let names = ["dev.json", "release.json", "host.json"];
    for name in names {
        match fs::symlink_metadata(out.join(name)) {
            Ok(meta) => {
                require(
                    meta.is_file(),
                    format!("invalid receipt destination: {name}"),
                )?;
                fs::hard_link(out.join(name), backups.path().join(name))?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(err.into()),
        }
    }
    for (index, name) in names.iter().enumerate() {
        if let Err(error) = fs::rename(stage.join(name), out.join(name)) {
            for prior in names[..index].iter().rev() {
                let backup = backups.path().join(prior);
                if backup.try_exists()? {
                    fs::rename(backup, out.join(prior))?;
                } else {
                    fs::remove_file(out.join(prior))?;
                }
            }
            return Err(error.into());
        }
    }
    Ok(())
}
fn main() -> Result {
    match Args::parse().action {
        Action::Run {
            framework,
            original,
            out,
        } => replay(&framework, &original, &out),
        Action::Verify {
            framework,
            target,
            reports,
        } => {
            let directory = fs::File::open(&reports)?;
            directory.lock_shared()?;
            let pins = check_pair(&reports, &expected(&framework)?, &target)?;
            let host = reports.join("host.json");
            if host.try_exists()? {
                let meta = load(&host)?;
                require(
                    meta["receipts"] == pins
                        && meta["rustHost"] == target
                        && meta["frameworkRevision"] == baseline_framework::revision(),
                    "host receipt does not bind the verified profiles",
                )?;
            }
            println!("{}", serde_json::to_string(&pins)?);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_publication_restores_previous_profiles() -> Result {
        let out = tempfile::tempdir()?;
        let stage = tempfile::tempdir_in(out.path())?;
        fs::write(out.path().join("release.json"), b"old release")?;
        fs::write(out.path().join("host.json"), b"old host")?;
        for name in ["dev.json", "release.json"] {
            fs::write(stage.path().join(name), b"new")?;
        }
        assert!(publish(stage.path(), out.path()).is_err());
        assert!(!out.path().join("dev.json").try_exists()?);
        assert_eq!(fs::read(out.path().join("release.json"))?, b"old release");
        assert_eq!(fs::read(out.path().join("host.json"))?, b"old host");
        Ok(())
    }
}
