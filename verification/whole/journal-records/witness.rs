//! Link the independently authored public record witness to Cargo's reported artifacts.
use public_data_qualification::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

pub fn library(rows: &[Value], name: &str) -> Result<PathBuf> {
    let mut found = Vec::new();
    for r in rows {
        if r["reason"] == "compiler-artifact" && r["target"]["name"] == name {
            for file in list(&r["filenames"])? {
                let f = text(file)?;
                if f.ends_with(".rlib") {
                    found.push(PathBuf::from(f));
                }
            }
        }
    }
    let [path]: [PathBuf; 1] = found
        .try_into()
        .map_err(|_| format!("ambiguous native artifact {name}"))?;
    Ok(path)
}
pub fn cargo_rows(raw: &[u8]) -> Result<Vec<Value>> {
    Ok(std::str::from_utf8(raw)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?)
}
pub fn journal_probe(output: &Path, rows: &[Value]) -> Result<Value> {
    let native = library(rows, "marmish_native")?;
    let serde = library(rows, "serde_json")?;
    let mut links = BTreeSet::new();
    for r in rows {
        if r["reason"] == "build-script-executed" {
            for l in list(&r["linked_paths"])? {
                links.insert(text(l)?);
            }
        }
    }
    let mut c = Command::new("rustc");
    c.args(["+1.97.1", "--edition=2024"])
        .arg(whole().join("journal-records/probe.rs"))
        .arg("-L")
        .arg(format!(
            "dependency={}",
            serde.parent().ok_or("invalid serde library")?.display()
        ))
        .arg("--extern")
        .arg(format!("marmish_native={}", native.display()))
        .arg("--extern")
        .arg(format!("serde_json={}", serde.display()));
    for l in links {
        c.arg("-L").arg(l);
    }
    run(c.arg("-o").arg(output))?;
    let lines =
        stdout(Command::new(output).current_dir(output.parent().ok_or("invalid journal probe")?))?;
    Ok(
        json!({"jsonLines":lines.lines().collect::<Vec<_>>(),"libraries":{"serde_json":sha(&serde)?,"marmish_native":sha(&native)?}}),
    )
}
