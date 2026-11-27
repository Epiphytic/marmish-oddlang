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
pub fn artifacts(bytes: &[u8], product: &Path, target: &Path, host: &str) -> Result<Artifacts> {
    let rows: Vec<Value> = std::str::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.last() == Some(&json!({"reason":"build-finished","success":true})),
        "incomplete Cargo build",
    )?;
    let mut roles = json!({});
    let mut links = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut libs: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    for row in rows {
        if row["reason"] == "build-script-executed" {
            for p in list(&row["linked_paths"])? {
                links.insert(text(p)?.to_owned());
            }
            continue;
        }
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let paths = list(&row["filenames"])?
            .iter()
            .map(|v| Ok(PathBuf::from(text(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let name = text(&row["target"]["name"])?;
        let package = name.replace('_', "-");
        if row["target"]["kind"] == json!(["lib"])
            && PKGS.iter().any(|n| package == format!("marmish-{n}"))
        {
            let dir = product.join(&package);
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
