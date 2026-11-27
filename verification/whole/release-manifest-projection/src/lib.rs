//! Reusable compiler observations for the standalone and integrated release parser.
pub mod shapes;
use public_data_qualification::{Result, assembly, capture, load, require};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn run(cmd: &mut Command) -> Result<Vec<u8>> {
    let output = capture(cmd, 1200, None)?;
    require(
        output.status.success(),
        format!(
            "{cmd:?}\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(output.stdout)
}
pub fn cargo() -> Command {
    let mut cmd = Command::new("cargo");
    assembly::build_env(&mut cmd);
    cmd.arg("+1.97.1");
    cmd
}
pub fn rustdoc(root: &Path, target: &Path) -> Result<PathBuf> {
    run(cargo()
        .current_dir(root)
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "rustdoc",
            "--release",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(target)
        .args([
            "-p",
            "marmish-native",
            "--lib",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
            "--document-private-items",
        ]))?;
    Ok(target.join("doc/marmish_native.json"))
}

pub fn compare_records(
    original_doc: &Path,
    generated_doc: &Path,
    generated_source: &str,
    graph: &Value,
    binding: &Value,
) -> Result<Value> {
    let expected = shapes::shape(
        &load(original_doc)?,
        "crates/marmish-native/src/native_manifest.rs",
    )?;
    require(
        expected == shapes::shape(&load(generated_doc)?, generated_source)?
            && expected == shapes::declared(graph, binding)?,
        "native record shape mismatch",
    )?;
    Ok(expected)
}
