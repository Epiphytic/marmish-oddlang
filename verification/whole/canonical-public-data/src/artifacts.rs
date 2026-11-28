//! Audit the original source pins and select the sealed seven-member Cargo artifact closure.
use crate::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};
pub const PKGS: [&str; 7] = [
    "core",
    "engine",
    "mdk",
    "native",
    "prototype",
    "stream",
    "vault",
];
pub fn source_pin(original: &Path) -> Result<Value> {
    checkout(original, ORIGINAL, false)?;
    let tree = stdout(
        Command::new("git")
            .current_dir(original)
            .args(["rev-parse", "HEAD^{tree}"]),
    )?;
    let mut files = json!({});
    for p in ["Cargo.toml".into(), "Cargo.lock".into()]
        .into_iter()
        .chain(PKGS.map(|n| format!("crates/marmish-{n}/Cargo.toml")))
    {
        files[&p] = sha(&original.join(&p))?.into();
    }
    Ok(json!({"revision":ORIGINAL,"tree":tree.trim(),"files":files}))
}
fn one(mut paths: impl Iterator<Item = PathBuf>) -> Result<PathBuf> {
    let value = paths.next().ok_or("missing native artifact")?;
    require(paths.next().is_none(), "ambiguous native artifact")?;
    Ok(value)
}
pub struct Artifacts {
    pub roles: Value,
    pub links: BTreeSet<String>,
    pub files: BTreeMap<String, String>,
    pub serde: PathBuf,
    pub serde_json: PathBuf,
}
pub fn cargo_rows(bytes: &[u8]) -> Result<Vec<Value>> {
    let rows: Vec<Value> = std::str::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.last() == Some(&json!({"reason":"build-finished","success":true})),
        "incomplete Cargo build",
    )?;
    Ok(rows)
}

pub fn selected_libraries(
    rows: &[Value],
    package_root: &Path,
    target: &Path,
    packages: &[&str],
) -> Result<(Value, BTreeSet<String>)> {
    let mut roles = json!({});
    let mut links = BTreeSet::new();
    for row in rows {
        if row["reason"] == "build-script-executed" {
            for link in list(&row["linked_paths"])? {
                links.insert(text(link)?.to_owned());
            }
        }
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let package = text(&row["target"]["name"])?.replace('_', "-");
        if row["target"]["kind"] == json!(["lib"]) && packages.contains(&package.as_str()) {
            let paths = list(&row["filenames"])?
                .iter()
                .map(|v| Ok(PathBuf::from(text(v)?)))
                .collect::<Result<Vec<_>>>()?;
            let dir = package_root.join(&package);
            require(
                Path::new(text(&row["manifest_path"])?).canonicalize()? == dir.join("Cargo.toml")
                    && Path::new(text(&row["target"]["src_path"])?)
                        .canonicalize()?
                        .starts_with(&dir)
                    && row["profile"]["opt_level"] == "3"
                    && row["profile"]["test"] == false,
                "wrong generated native library provenance",
            )?;
            let artifact = |ext: &str| -> Result<PathBuf> {
                let p = one(paths
                    .iter()
                    .filter(|p| p.extension().is_some_and(|e| e == ext))
                    .cloned())?
                .canonicalize()?;
                require(p.starts_with(target), "library outside target directory")?;
                Ok(p)
            };
            require(
                roles.get(&package).is_none(),
                "duplicate generated native library",
            )?;
            roles[package] = json!([
                artifact("rmeta")?,
                artifact("rlib")?,
                row["features"],
                target.join("release/deps"),
                row["fresh"]
            ]);
        }
    }
    require(
        roles.as_object().is_some_and(|v| v.len() == packages.len()),
        "incomplete native library inventory",
    )?;
    Ok((roles, links))
}
pub fn selected_build(
    bytes: &[u8],
    package_root: &Path,
    target: &Path,
    packages: &[&str],
) -> Result<(Value, BTreeSet<String>)> {
    selected_libraries(&cargo_rows(bytes)?, package_root, target, packages)
}
pub fn artifacts(bytes: &[u8], product: &Path, target: &Path, host: &str) -> Result<Artifacts> {
    let rows = cargo_rows(bytes)?;
    let (roles, links) = selected_libraries(&rows, product, target, &crate::product::PACKAGES)?;
    let mut files = BTreeMap::new();
    let mut libs: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    for row in rows {
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let paths = list(&row["filenames"])?
            .iter()
            .map(|v| Ok(PathBuf::from(text(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let name = text(&row["target"]["name"])?;
        for p in paths {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "rlib"
                && ["serde", "serde_json"].contains(&name)
                && row["target"]["kind"] == json!(["lib"])
                && row["profile"]["opt_level"] == "3"
                && p.starts_with(target.join(host).join("release"))
            {
                libs.entry(name.into()).or_default().insert(p.clone());
            }
            if ["rmeta", "rlib", if host == LINUX { "so" } else { "dylib" }].contains(&ext) {
                if host == LINUX {
                    require(p == p.canonicalize()?, "library closure contains symlink")?;
                }
                files.insert(
                    p.strip_prefix(target)?
                        .to_str()
                        .ok_or("non-UTF-8 path")?
                        .into(),
                    sha(&p)?,
                );
            }
        }
    }
    require(
        roles.as_object().is_some_and(|v| v.len() == PKGS.len()) && !files.is_empty(),
        "incomplete native member closure",
    )?;
    Ok(Artifacts {
        roles,
        links,
        files,
        serde: one(libs.remove("serde").unwrap_or_default().into_iter())?,
        serde_json: one(libs.remove("serde_json").unwrap_or_default().into_iter())?,
    })
}
