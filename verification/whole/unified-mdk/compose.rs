//! Append the qualified MDK member to the five-member source-free workspace.
use crate::*;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
const BASE: &str = "workspace-v7-recovered-vault-engine-journal.json";
const RELEASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk";
const MDK: &str = "mdk-public-data-package.json";
const MEMBER: &str = "marmish-mdk";

pub(crate) fn packages(raw: &str) -> Result<Vec<toml::Value>> {
    let document: toml::Table = toml::from_str(raw)?;
    Ok(document
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing Cargo packages")?
        .clone())
}
fn combined_lock(old: &str, mdk: &str) -> Result<String> {
    let expected = toml::Value::try_from(json!({"name":MEMBER,"version":"0.1.0"}))?;
    require(
        packages(mdk)? == [expected]
            && packages(old)?
                .iter()
                .all(|p| p.get("name").and_then(toml::Value::as_str) != Some(MEMBER)),
        "expected a single, dependency-free MDK package",
    )?;
    let entry = "[[package]]\nname = \"marmish-native\"\n";
    require(
        old.matches(entry).count() == 1,
        "expected one existing native package entry",
    )?;
    Ok(old.replacen(
        entry,
        &format!("[[package]]\nname = \"{MEMBER}\"\nversion = \"0.1.0\"\n\n{entry}"),
        1,
    ))
}
pub fn compose(root: &Path, check: bool) -> Result<PathBuf> {
    compose_member(root, check, BASE, RELEASE, MDK, MEMBER, combined_lock)
}
pub(crate) fn compose_member(
    root: &Path,
    check: bool,
    base: &str,
    release: &str,
    descriptor: &str,
    member: &str,
    combine: fn(&str, &str) -> Result<String>,
) -> Result<PathBuf> {
    let mut manifest = load(&root.join(base))?;
    let load_lock = |p: &serde_json::Value, msg: &str| -> Result<String> {
        let rel = text(&p["path"])?;
        require(pin(root, rel)? == *p, msg)?;
        Ok(fs::read_to_string(root.join(rel))?)
    };
    let original = load_lock(
        &manifest["cargoLock"],
        "predecessor Cargo lock does not match its pin",
    )?;
    let standalone = load_lock(
        &load(&root.join(descriptor))?["cargoLock"],
        "standalone package Cargo lock does not match its pin",
    )?;
    let lock = combine(&original, &standalone)?;
    let lock_rel = format!("{release}/Cargo.lock");
    let lock_path = root.join(&lock_rel);
    let manifest_path = root.join(format!("{release}.json"));
    if check {
        require(
            fs::read(&lock_path)? == lock.as_bytes(),
            "workspace Cargo lock differs beyond the added member",
        )?;
    } else {
        fs::create_dir_all(root.join(release))?;
        fs::write(&lock_path, lock)?;
    }
    manifest["cargoLock"] = pin(root, &lock_rel)?;
    manifest["members"][member] = json!({"manifest":pin(root,descriptor)?});
    let bytes = json_bytes(&manifest)?;
    if check {
        require(
            fs::read(&manifest_path)? == bytes,
            "workspace manifest differs beyond the added member",
        )?;
    } else {
        fs::write(&manifest_path, bytes)?;
    }
    Ok(manifest_path)
}
