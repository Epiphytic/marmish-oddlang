//! Verify and build the reviewed framework used by the original qualification runners.
use crate::{Result, assembly::checkout, require, run};
use std::{fs, io::Write, path::Path, process::Command};

pub fn revision() -> &'static str {
    serde_json::from_str(include_str!("../../baseline-framework.json"))
        .expect("valid reviewed revision")
}

pub fn verify(root: &Path) -> Result<()> {
    checkout(root, revision(), false)
}

pub fn build(root: &Path, destination: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    verify(&root)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    require(
        !parent.starts_with(&root),
        "use an output outside the framework checkout",
    )?;
    require(!destination.try_exists()?, "compiler output already exists")?;
    let scratch = tempfile::tempdir()?;
    let mut cargo = Command::new("cargo");
    for key in [
        "RUSTC",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        cargo.env_remove(key);
    }
    run(cargo
        .current_dir(&root)
        .args([
            "+1.97.1",
            "build",
            "--offline",
            "--locked",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(scratch.path()))?;
    verify(&root)?;
    let mut compiled = fs::File::open(scratch.path().join("debug/oddlang"))?;
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut compiled, &mut output)?;
    output.flush()?;
    output
        .as_file()
        .set_permissions(compiled.metadata()?.permissions())?;
    output.persist_noclobber(destination)?;
    Ok(())
}
