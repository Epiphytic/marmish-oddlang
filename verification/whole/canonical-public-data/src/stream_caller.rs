//! Compile an external caller against only the specified stream library.
use crate::{Result, assembly::build_env, run};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub fn build(
    work: &Path,
    dependency: &Path,
    lock: &Path,
    driver: &[u8],
    name: &str,
    edition: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(work.join("src"))?;
    fs::write(work.join("src/main.rs"), driver)?;
    let manifest = format!(
        "[workspace]\n[package]\nname='{name}'\nversion='0.1.0'\nedition='{edition}'\n[dependencies]\nmarmish-stream={{path={}}}\n",
        toml::Value::String(dependency.to_str().ok_or("non-UTF-8 crate path")?.into())
    );
    fs::write(work.join("Cargo.toml"), manifest)?;
    fs::copy(lock, work.join("Cargo.lock"))?;
    run(build_env(Command::new("cargo").current_dir(work).args([
        "+1.97.1",
        "metadata",
        "--offline",
        "--format-version",
        "1",
    ])))?;
    run(build_env(
        Command::new("cargo")
            .current_dir(work)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--target-dir",
            ])
            .arg(work.join("target")),
    ))?;
    Ok(work.join("target/release").join(name))
}
