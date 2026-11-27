//! Build and invoke a local stream wire caller against Cargo's actual compiler artifacts.
use public_data_qualification::{Result, assembly, json_bytes, require, run, stdout, text};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn check_original(path: &Path) -> Result<Vec<String>> {
    assembly::checkout(path, assembly::ORIGINAL, false)?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        rust.lines()
            .any(|l| l == format!("commit-hash: {}", assembly::COMPILER)),
        "unexpected rustc revision",
    )?;
    Ok(rust.lines().map(str::to_owned).collect())
}
pub fn original_tree(path: &Path) -> Result<String> {
    Ok(stdout(
        Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "HEAD^{tree}"]),
    )?
    .trim()
    .into())
}
pub type Environment = BTreeMap<OsString, OsString>;
fn command(name: &str, env: Option<&Environment>) -> Command {
    let mut cmd = Command::new(name);
    if let Some(env) = env {
        cmd.env_clear().envs(env);
    }
    cmd
}
pub fn compare_caller(
    root: &Path,
    target: &Path,
    work: &Path,
    caller: &Path,
    required: &[&str],
    edition: &str,
    env: Option<&Environment>,
) -> Result<Vec<u8>> {
    let build = stdout(
        command("cargo", env)
            .current_dir(root)
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "--release",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target)
            .args(["-p", "marmish-stream"]),
    )?;
    let mut libraries = BTreeMap::<String, PathBuf>::new();
    for line in build.lines() {
        let event: Value = serde_json::from_str(line)?;
        if event["reason"] != "compiler-artifact" {
            continue;
        }
        let name = text(&event["target"]["name"])?;
        if !required.contains(&name) {
            continue;
        }
        let paths: Vec<_> = event["filenames"]
            .as_array()
            .ok_or("missing filenames")?
            .iter()
            .filter_map(Value::as_str)
            .filter(|f| f.ends_with(".rlib"))
            .collect();
        if let [path] = paths.as_slice() {
            libraries.insert(name.into(), (*path).into());
        }
    }
    require(
        libraries.len() == required.len(),
        "missing native compiler artifacts",
    )?;
    let binary = work.join("consumer");
    let mut rustc = command("rustc", env);
    rustc.arg("+1.97.1").arg(format!("--edition={edition}"));
    if edition == "2024" {
        rustc.arg("-Dwarnings");
    }
    rustc.arg(caller).arg("-L").arg(format!(
        "dependency={}",
        target.join("release/deps").display()
    ));
    for (name, path) in libraries {
        rustc
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    run(rustc.arg("-o").arg(&binary))?;
    let mut cmd = Command::new(binary);
    if let Some(env) = env {
        cmd.env_clear().envs(env);
    }
    run(&mut cmd)
}
pub fn write_receipt(path: &Path, mut record: Value) -> Result {
    record.sort_all_objects();
    Ok(fs::write(path, json_bytes(&record)?)?)
}
