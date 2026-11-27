//! Bind the release-info selector to a specific upstream source checkout.
use public_data_qualification::{Result, json_bytes, list, load, pin, run, text, whole};
use std::{fs, path::Path, process::Command};

pub fn recover(cli: &Path, source: &Path, output: &Path, selection: &Path) -> Result {
    let mut config = load(&whole().join("release-info-structure/selection.json"))?;
    config["sources"] = list(&config["sources"])?
        .iter()
        .map(|row| pin(source, text(&row["path"])?))
        .collect::<Result<Vec<_>>>()?
        .into();
    fs::write(selection, json_bytes(&config)?)?;
    run(Command::new(cli)
        .env("RUSTUP_TOOLCHAIN", "1.97.1")
        .args(["recover", "build-report"])
        .arg(source)
        .arg(selection)
        .arg(output))?;
    Ok(())
}
