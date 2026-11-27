//! Compile the frozen consumer imports against the measured range library build.
use clap::Parser;
use public_data_qualification::{
    artifacts::artifacts,
    assembly::{ORIGINAL, bytes_sha},
    imports,
    range::{hashed, host, keys, library_closure},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    retained_build: PathBuf,
    receipt: PathBuf,
}

fn diagnostics(row: &Value) -> Result<Vec<&str>> {
    list(&row["diagnostics"])?
        .iter()
        .map(|d| text(&d["code"]))
        .collect()
}

fn compare(name: &str, before: &Value, after: &Value) -> Result<Value> {
    for key in [
        "inventorySha256",
        "inventoryLockSha256",
        "inventoryEvidenceSha256",
    ] {
        require(
            before[key] == after[key],
            format!("upstream import inventory changed: {key}"),
        )?;
    }
    let prior = list(&before["imports"])?;
    let fresh = list(&after["imports"])?;
    require(prior.len() == fresh.len(), "import witness count changed")?;
    let mut probes = json!({});
    for (a, b) in prior.iter().zip(fresh) {
        let spelling = text(&b["spelling"])?;
        require(
            a["spelling"] == b["spelling"]
                && a["originalInventoryKind"] == b["originalInventoryKind"],
            "different import witnesses",
        )?;
        let codes = diagnostics(b)?;
        if name == "engine" && spelling == "MissingRange" {
            require(
                a["compiles"] == false && b["compiles"] == true,
                "expected MissingRange to become importable",
            )?;
        } else {
            require(
                a["compiles"] == b["compiles"] && diagnostics(a)? == codes,
                format!("changed import outcome: {name}::{spelling}"),
            )?;
        }
        probes[spelling] = json!({"compiled":b["compiles"],"diagnostics":codes});
    }
    Ok(json!({"accepted":after["importable"],"attempted":keys(&probes)?.len(),"imports":probes}))
}

fn check(original: &Path, build: &Path) -> Result<Value> {
    let (host, rustc) = host()?;
    let mut receipts = json!({});
    let file = |name: &str| whole().join(name).join(format!("{host}.json"));
    for name in [
        "canonical-public-data",
        "canonical-range-data",
        "canonical-recovered-range",
        "gap-range-role-workspace",
    ] {
        receipts[name] = json!(sha(&file(name))?);
    }
    let measured = load(&file("canonical-range-data"))?;
    let recovered = load(&file("canonical-recovered-range"))?;
    let role = load(&file("gap-range-role-workspace"))?;
    let raw = fs::read(build.join("cargo-artifacts.jsonl"))?;
    let product = build.join("candidate");
    let target = build.join("target");
    require(
        rustc == measured["rustc"]
            && bytes_sha(&raw) == measured["cargoArtifactLogSha256"]
            && sha(&product.join("workspace.lock"))?
                == measured["workspace"]["workspaceLockSha256"]
            && recovered["priorNativeReceiptSha256"] == receipts["canonical-range-data"]
            && role["predecessorReceiptSha256"] == receipts["canonical-recovered-range"],
        "native library build or successor evidence differs",
    )?;
    let files = library_closure(&raw, &target, &host)?;
    require(
        keys(&files)?.len() == measured["cargoLibraryClosureEntries"]
            && hashed(&files)? == measured["cargoLibraryClosureSha256"],
        "native Cargo libraries differ from qualified build",
    )?;
    require(
        stdout(
            Command::new("git")
                .current_dir(original)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == ORIGINAL,
        "wrong upstream revision",
    )?;
    let roles = artifacts(&raw, &product, &target, &host)?.roles;
    let baseline = load(&file("canonical-public-data"))?;
    let mut summary = json!({});
    for (name, count) in imports::COUNT {
        let observed = imports::library(
            name,
            count,
            &host,
            &roles[format!("marmish-{name}")],
            original,
        )?;
        summary[name] = compare(name, &baseline["imports"]["libraries"][name], &observed)?;
    }
    require(
        files == library_closure(&raw, &target, &host)?,
        "native library closure changed during import audit",
    )?;
    Ok(
        json!({"host":host,"rustc":rustc,"runnerSha256":bytes_sha(include_bytes!("imports.rs")),
        "receiptDigests":receipts,"measuredLibraryWorkspace":measured["workspace"]["workspaceLockSha256"],
        "equivalentSourceWorkspace":role["workspaceLockSha256"],"libraries":summary}),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let receipt = check(
        &args.original.canonicalize()?,
        &args.retained_build.canonicalize()?,
    )?;
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
