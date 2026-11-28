//! Recover the linked range from Rust and stage its pinned native package inputs.
#[path = "../../../gap-range-compact-source-api/verify.rs"]
mod source_api;
#[path = "../../../gap-range-endpoint/verify.rs"]
mod endpoint;
#[path = "../../../gap-range-assembly/verify.rs"]
mod derived;
#[path = "../../../gap-range-recovery/evidence.rs"]
mod evidence;
#[path = "../../../gap-range-recovery/qualify.rs"]
mod qualify;
#[path = "../../../gap-range-recovery/sealed.rs"]
mod sealed;
#[path = "../../../gap-range-recovery/witnesses.rs"]
mod witnesses;
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, json_bytes, load, pin, portable, product::pinned, require, run, sha, text,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const MODEL: &str = "crates/marmish-engine/src/model.rs";
const ID: &str = "urn:marmish:engine:output-gap:";
const LEGACY: &str = "77c4c6272bf317e11cf9a2783eb7b00e3fcca0f0";
const REASON: &str = "cf2cd3940b790740d6e090bdcbf8702681c656e4";
const RECORD_DIR: &str = "engine-gap-range-recovered-v1";
const REASON_DIR: &str = "engine-gap-reason-recovered-v1";
const PACKAGE: &str = "engine-gap-range-recovered-package.json";

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Verify compact recovery-based package assembly against the sealed explicit package.
    QualifyDerived(derived::Args),
    /// Check the compact package against the native range endpoint source API.
    QualifySourceApi(derived::Args),
    /// Replay native endpoint profiles and the optional TypeScript caller.
    QualifyEndpoint(endpoint::Args),
    /// Stage the compact manifest and its pinned recovery inputs at a fresh destination.
    StageDerived { output: PathBuf },
    /// Recover both source variants and qualify their generated native packages.
    Qualify(qualify::Args),
    /// Verify native caller receipts using the rustc -vV lines supplied as JSON on stdin.
    CheckWitnesses {
        original: PathBuf,
        mutation: PathBuf,
        host: String,
    },
    /// Verify both collected Rustdoc files and print their checked provenance receipt.
    CheckEvidence {
        directory: PathBuf,
        original: PathBuf,
        mutation: PathBuf,
        host: String,
    },
    /// Emit the source selection and checked recovered graph, overlay and lock.
    Recover {
        compiler: PathBuf,
        source: PathBuf,
        rustdoc: PathBuf,
        output: PathBuf,
    },
    /// Compose the recovered record with the independently pinned reason and Cargo lock.
    Stage { recovered: PathBuf, output: PathBuf },
}
fn write_json(path: &Path, mut value: Value) -> Result {
    value.sort_all_objects();
    fs::write(path, json_bytes(&value)?)?;
    Ok(())
}
fn recover(compiler: &Path, source: &Path, doc: &Path, work: &Path) -> Result {
    let document = load(doc)?;
    let entries = document["index"]
        .as_object()
        .ok_or("missing rustdoc index")?;
    let files = entries
        .values()
        .filter(|i| {
            i["name"] == "MissingRange"
                && i["inner"].get("struct").is_some()
                && i["crate_id"] == 0
                && !i["span"].is_null()
        })
        .map(|i| text(&i["span"]["filename"]))
        .collect::<Result<BTreeSet<_>>>()?;
    require(
        files == BTreeSet::from([MODEL]),
        "unexpected compiled declaration source path",
    )?;
    let root = source.with_file_name("lib.rs");
    let selection = work.join("selection.json");
    write_json(
        &selection,
        json!({
            "format": "oddlang-serde-public-record-selection-v1",
            "sourceFilename": MODEL,
            "sourceSha256": sha(source)?,
            "rustdocSha256": sha(doc)?,
            "crateName": "marmish_engine",
            "crateRootSha256": sha(&root)?,
            "moduleId": format!("{ID}range-module"),
            "moduleName": "model",
            "record": {
                "name": "MissingRange",
                "id": format!("{ID}range"),
                "fields": {
                    "start": format!("{ID}range:start"),
                    "end": format!("{ID}range:end"),
                    "reason": format!("{ID}range:reason")
                }
            },
            "choices": [{"name": "MissingReason", "id": format!("{ID}reason")}]
        }),
    )?;
    let out = work.join("recovered");
    run(qualify::tool(compiler)
        .args(["recover", "serde-public-record"])
        .arg(source)
        .arg(doc)
        .arg(&selection)
        .arg(&out))?;
    let lock = load(&out.join("recovery.lock"))?;
    require(
        lock["format"] == "oddlang-serde-public-record-recovery-lock-v1",
        "unexpected recovery lock format",
    )?;
    for (key, path) in [
        ("selection", selection.as_path()),
        ("source", source),
        ("crateRoot", &root),
        ("rustdoc", doc),
        ("graph", &out.join("graph.json")),
        ("overlay", &out.join("rust.json")),
    ] {
        require(
            lock[format!("{key}Sha256")] == sha(path)?,
            format!("wrong {key} recovery digest"),
        )?;
    }
    Ok(())
}
fn stage(recovered: &Path, inputs: &Path) -> Result {
    let recovery = load(&recovered.join("recovery.lock"))?;
    require(
        recovery["format"] == "oddlang-serde-public-record-recovery-lock-v1",
        "unexpected range recovery lock",
    )?;
    let base = portable();
    let authored = base.join("engine-gap-range-package.json");
    pinned(&authored, LEGACY)?;
    let mut manifest = load(&authored)?;
    for (directory, index) in [(REASON_DIR, 0), (RECORD_DIR, 1)] {
        fs::create_dir(inputs.join(directory))?;
        for (key, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
            let rel = format!("{directory}/{file}");
            let source = if index == 0 {
                let src = base.join(&rel);
                pinned(&src, REASON)?;
                src
            } else {
                recovered.join(file)
            };
            fs::copy(source, inputs.join(&rel))?;
            let staged = pin(inputs, &rel)?;
            if directory == RECORD_DIR {
                require(
                    staged["sha256"] == recovery[format!("{key}Sha256")],
                    format!("recovered {key} differs from its lock"),
                )?;
            }
            manifest["components"][index][key] = staged;
        }
    }
    let relative = text(&manifest["cargoLock"]["path"])?;
    require(
        pin(&base, relative)? == manifest["cargoLock"],
        "wrong Cargo lock pin",
    )?;
    let dest = inputs.join(relative);
    fs::create_dir_all(dest.parent().ok_or("missing lock parent")?)?;
    fs::copy(base.join(relative), dest)?;
    write_json(&inputs.join(PACKAGE), manifest)
}
fn publish(output: PathBuf, prepare: impl FnOnce(&Path) -> Result) -> Result {
    let output = std::path::absolute(output)?;
    let parent = output.parent().ok_or("missing output parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::tempdir_in(parent)?;
    prepare(temp.path())?;
    renameat_with(CWD, temp.path(), CWD, output, RenameFlags::NOREPLACE)?;
    Ok(())
}
fn main() -> Result {
    match Args::parse().action {
        Action::QualifyEndpoint(args) => endpoint::check(args),
        Action::QualifySourceApi(args) => derived::run_qualification(args, source_api::FRAMEWORK,None,"product/package.lock", source_api::verify),
        Action::QualifyDerived(args) => derived::qualify(args),
        Action::StageDerived { output } => publish(output, derived::stage),
        Action::Qualify(args) => qualify::qualify(args),
        Action::CheckWitnesses {
            original,
            mutation,
            host,
        } => {
            let rustc = serde_json::from_reader(std::io::stdin().lock())?;
            println!("{}", witnesses::check(&original, &mutation, &host, &rustc)?);
            Ok(())
        }
        Action::CheckEvidence {
            directory,
            original,
            mutation,
            host,
        } => {
            println!(
                "{}",
                evidence::check(&directory, &original, &mutation, &host)?
            );
            Ok(())
        }
        Action::Recover {
            compiler,
            source,
            rustdoc,
            output,
        } => publish(output, |work| recover(&compiler, &source, &rustdoc, work)),
        Action::Stage { recovered, output } => publish(output, |work| stage(&recovered, work)),
    }
}
