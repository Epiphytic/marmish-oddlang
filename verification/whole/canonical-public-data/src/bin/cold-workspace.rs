//! Replay the v5 release from an empty Cargo home, then build and check it offline.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{checkout, inventory, stage},
    capture, json_bytes, load, portable,
    product::check_members,
    require, run, sha, stdout, whole,
};
use serde_json::json;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "e2ec7338dcf141f95d2f1e24d9bee20cc1cec44b";
const MANIFEST: &str = "workspace-v5.json";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    receipt: PathBuf,
}

fn main() -> Result {
    let args = Args::parse();
    let source = args.framework.canonicalize()?;
    checkout(&source, FRAMEWORK, false)?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let inputs = root.join("inputs");
    stage(&portable().join(MANIFEST), &inputs)?;
    let files = inventory(&inputs)?;

    let compiler_target = root.join("compiler");
    run(Command::new("cargo")
        .current_dir(&source)
        .args([
            "+1.96.1",
            "build",
            "--bin",
            "oddlang",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    let cli = root.join("oddlang");
    fs::rename(compiler_target.join("debug/oddlang"), &cli)?;
    fs::remove_dir_all(compiler_target)?;
    let sysroot = stdout(Command::new("rustc").args(["+1.97.1", "--print", "sysroot"]))?;
    let bin = Path::new(sysroot.trim()).join("bin");
    let search = env::join_paths([bin.as_path(), Path::new("/usr/bin"), Path::new("/bin")])?;
    let home = root.join("home");
    let tmp = root.join("tmp");
    let cold_cache = root.join("cold-cargo");
    let release_cache = root.join("release-cargo");
    for dir in [&home, &tmp, &cold_cache] {
        fs::create_dir(dir)?;
    }

    let command = |program: &Path, cache: &Path, offline: bool| {
        let mut cmd = Command::new(program);
        cmd.current_dir(&root)
            .env_clear()
            .env("HOME", &home)
            .env("TMPDIR", &tmp)
            .env("CARGO_HOME", cache)
            .env("PATH", &search)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        if offline {
            cmd.env("CARGO_NET_OFFLINE", "true");
        }
        cmd
    };
    let build = |cache: &Path, output: &Path, offline: bool| {
        let mut cmd = command(&cli, cache, offline);
        cmd.args(["build", "native-workspace", "rust"])
            .arg(inputs.join(MANIFEST))
            .arg(output);
        cmd
    };
    let cold = root.join("cold");
    let control = capture(&mut build(&cold_cache, &cold, false), 600, None)?;
    require(
        !control.status.success()
            && String::from_utf8_lossy(&control.stderr).contains("offline")
            && !cold.try_exists()?,
        "cold-cache offline control failed",
    )?;
    fs::create_dir(&release_cache)?;
    let initial = fs::read_dir(&release_cache)?.count();
    require(initial == 0, "nonempty acquisition cache")?;
    let connected = root.join("connected");
    run(build(&release_cache, &connected, false).arg("--fetch-locked"))?;
    let product = root.join("offline");
    run(&mut build(&release_cache, &product, true))?;
    let pin = portable().join("workspace-v5/workspace.lock");
    let expected = fs::read(&pin)?;
    let lock = load(&pin)?;
    for dir in [&connected, &product] {
        require(
            fs::read(dir.join("workspace.lock"))? == expected,
            "workspace lock differs from v5 release",
        )?;
        check_members(dir, &lock, true)?;
    }
    run(command(&bin.join("cargo"), &release_cache, true)
        .current_dir(&product)
        .args(["build", "--workspace", "--release", "--locked", "--offline"]))?;
    let release = product.join("target/release");
    let observed = public_data_qualification::cli_transcript::collect(&release)?;
    public_data_qualification::cli_transcript::verify(
        &observed,
        &load(&whole().join("cli/transcript.json"))?,
    )?;
    let mut binaries = json!({});
    let mut cases = json!({});
    for (role, rows) in observed.as_object().ok_or("invalid transcript")? {
        binaries[role] = sha(&release.join(role))?.into();
        cases[role] = rows
            .as_array()
            .ok_or("invalid transcript rows")?
            .len()
            .into();
    }
    let mut receipt = json!({"framework": FRAMEWORK, "compilerSha256":sha(&cli)?,
        "rustc":stdout(command(&bin.join("rustc"),&release_cache,true).arg("-vV"))?.trim(),
        "workspaceLockSha256":sha(&pin)?, "inputs":files, "initialAcquisitionCacheFiles":initial,
        "offlineColdRejected":true, "offlineReleaseBinaries":binaries,"parserCases":cases});
    receipt.sort_all_objects();
    let mut file = tempfile::NamedTempFile::new_in(
        args.receipt
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    file.write_all(&json_bytes(&receipt)?)?;
    file.persist(args.receipt)?;
    Ok(())
}
