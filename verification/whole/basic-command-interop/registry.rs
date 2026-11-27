//! Stage Cargo's sparse index and only the archives named in the release locks.
use public_data_qualification::{Result, require, sha};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn copy_index(source: &Path, target: &Path) -> Result {
    require(
        source.symlink_metadata()?.is_dir(),
        "expected ordinary registry index directory",
    )?;
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        let kind = entry.file_type()?;
        let output = target.join(name);
        if kind.is_dir() {
            copy_index(&entry.path(), &output)?;
        } else {
            require(kind.is_file(), "unexpected registry index entry")?;
            fs::copy(entry.path(), output)?;
        }
    }
    Ok(())
}

fn archives(locks: &[PathBuf]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for lock in locks {
        let document: toml::Table = toml::from_str(&fs::read_to_string(lock)?)?;
        let packages = document
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing locked packages")?;
        for package in packages {
            let Some(checksum) = package.get("checksum") else {
                continue;
            };
            let field = |key| {
                package
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .ok_or("invalid locked package field")
            };
            require(
                field("source")? == "registry+https://github.com/rust-lang/crates.io-index",
                "unexpected registry",
            )?;
            let filename = format!("{}-{}.crate", field("name")?, field("version")?);
            require(
                Path::new(&filename)
                    .file_name()
                    .and_then(|name| name.to_str())
                    == Some(filename.as_str()),
                "invalid archive filename",
            )?;
            let checksum = checksum.as_str().ok_or("invalid archive checksum")?;
            if let Some(previous) = result.insert(filename, checksum.to_owned()) {
                require(previous == checksum, "inconsistent lock checksums")?;
            }
        }
    }
    Ok(result)
}

pub fn stage(locks: &[PathBuf], source: &Path, destination: &Path) -> Result<Value> {
    let wanted = archives(locks)?;
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    require(
        !parent.starts_with(&source),
        "destination inside registry source",
    )?;
    let stage = tempfile::tempdir_in(parent)?;
    copy_index(&source.join("index"), &stage.path().join("index"))?;
    let mut caches = Vec::new();
    if !wanted.is_empty() {
        let cache = source.join("cache");
        require(cache.symlink_metadata()?.is_dir(), "invalid registry cache")?;
        for entry in fs::read_dir(cache)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("index.crates.io-"))
            {
                require(entry.file_type()?.is_dir(), "invalid cache directory")?;
                caches.push(entry.path());
            }
        }
    }
    for (filename, checksum) in &wanted {
        let mut archive = None;
        for cache in &caches {
            let candidate = cache.join(filename);
            match candidate.symlink_metadata() {
                Ok(metadata) => {
                    require(
                        metadata.is_file() && archive.is_none(),
                        format!("ambiguous or non-file archive: {filename}"),
                    )?;
                    archive = Some((candidate, cache));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        let (input, cache) =
            archive.ok_or_else(|| format!("missing cached archive: {filename}"))?;
        let out_dir = stage
            .path()
            .join("cache")
            .join(cache.file_name().ok_or("invalid cache name")?);
        fs::create_dir_all(&out_dir)?;
        let output = out_dir.join(filename);
        fs::copy(input, &output)?;
        require(
            sha(&output)? == *checksum,
            format!("cached archive checksum differs: {filename}"),
        )?;
    }
    renameat_with(CWD, stage.path(), CWD, destination, RenameFlags::NOREPLACE)?;
    Ok(serde_json::to_value(wanted)?)
}
