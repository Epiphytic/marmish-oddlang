//! Qualify the stream array graph using the pinned compiler, rustdoc and previous release.
mod compare;
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, assembly::checkout, cli_projection::REVISION, json_bytes, list, load, portable,
    require, run, sha, stdout, text, whole,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "ce22923ccc3f0f34b71c08525b455c16bc568d4e";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Compare {
        previous: PathBuf,
        current: PathBuf,
        rustdoc: PathBuf,
        receipt: PathBuf,
    },
    Replay {
        original: PathBuf,
        framework: PathBuf,
        output: PathBuf,
    },
}
fn write(path: &Path, mut data: Value) -> Result<()> {
    data.sort_all_objects();
    fs::write(path, json_bytes(&data)?)?;
    Ok(())
}
fn semantic_lock(mut lock: Value) -> Result<Value> {
    lock["pin"]
        .as_object_mut()
        .and_then(|p| p.remove("cargoConfigSha256"))
        .ok_or("missing Cargo configuration pin")?;
    Ok(lock)
}
fn unique_named(items: &Value, name: &str) -> Result<usize> {
    let mut matches = list(items)?
        .iter()
        .enumerate()
        .filter(|(_, v)| v["name"] == name);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("missing item {name}"))?;
    require(matches.next().is_none(), format!("ambiguous item {name}"))?;
    Ok(index)
}

fn replay(original: &Path, framework: &Path, output: &Path) -> Result<()> {
    checkout(original, REVISION, false)?;
    checkout(framework, FRAMEWORK, false)?;
    require(output.symlink_metadata().is_err(), "output already exists")?;
    let version = stdout(
        Command::new("rustc")
            .current_dir(original)
            .args(["+1.97.1", "-vV"]),
    )?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    require(
        version
            .lines()
            .any(|s| s == format!("commit-hash: {COMPILER}")),
        "unexpected compiler revision",
    )?;
    let archive = portable().join("workspace-shapes-v1");
    let release = load(&archive.join("release.json"))?;
    let mut cells = list(&release["cells"])?.iter().filter(|v| {
        v["package"] == "marmish-stream"
            && v["kind"] == "lib"
            && v["features"] == json!([])
            && v["triple"] == host
    });
    let cell = cells.next().ok_or("missing archived stream library")?;
    require(cells.next().is_none(), "ambiguous archived stream library")?;
    let digest = text(&cell["graphSha256"])?;
    require(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid archived graph pin",
    )?;
    let old = archive.join("graphs").join(format!("{digest}.json"));
    require(
        sha(&old)? == digest,
        "previous archived graph digest mismatch",
    )?;
    let tmp = tempfile::tempdir()?;
    let w = tmp.path();
    let profile = json!({"package":cell["package"],"targetKind":cell["kind"],"targetName":cell["target"],"targetTriple":host,
       "features":cell["features"],"defaultFeatures":cell["defaultFeatures"],"compilerCommit":COMPILER,
       "cargoLockSha256":sha(&original.join("Cargo.lock"))?,"adapter":framework.join("verification/rust-recovery/run.sh"),
       "adapterInputs":["Cargo.toml","Cargo.lock","driver.rs","shape_model.rs","shapes.rs"],"sourceRevision":REVISION});
    let selection = w.join("profile.toml");
    fs::write(&selection, toml::to_string(&profile)?)?;
    let target = w.join("tools");
    run(Command::new("cargo")
        .current_dir(framework)
        .args([
            "+1.96.1",
            "build",
            "--offline",
            "--locked",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&target))?;
    let shapes = w.join("shapes");
    run(Command::new(target.join("debug/oddlang"))
        .current_dir(original)
        .args(["recover", "rust-shapes"])
        .arg(original.join("Cargo.toml"))
        .arg("--profile")
        .arg(&selection)
        .arg("--out")
        .arg(&shapes))?;
    let docs = w.join("doc-build");
    run(Command::new("cargo")
        .current_dir(original)
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "+1.97.1",
            "rustdoc",
            "--offline",
            "--locked",
            "--release",
            "--lib",
            "-p",
            "marmish-stream",
            "--target",
            host,
            "--target-dir",
        ])
        .arg(&docs)
        .args([
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
            "--document-private-items",
        ]))?;
    let doc = load(&docs.join(host).join("doc/marmish_stream.json"))?;
    let graph = shapes.join("shapes.json");
    let mut current = load(&graph)?;
    let previous = load(&old)?;
    let mut report = compare::compare(&previous, &current, &doc)?;
    let i = unique_named(&current["declarations"], "Launch")?;
    let fields = &mut current["declarations"][i]["fields"];
    let j = unique_named(fields, "transfer")?;
    let transfer = &mut fields[j];
    let id = transfer["id"].clone();
    let count = &mut transfer["type"]["extent"]["count"];
    *count = json!(
        count
            .as_u64()
            .ok_or("missing transfer count")?
            .checked_add(1)
            .ok_or("extent overflow")?
    );
    let err = compare::compare(&previous, &current, &doc)
        .err()
        .ok_or("extent mutation accepted")?;
    require(
        err.to_string().contains("array element or extent mismatch"),
        err.to_string(),
    )?;
    report["upstream"] = REVISION.into();
    report["framework"] = FRAMEWORK.into();
    report["host"] = host.into();
    report["previousGraphSha256"] = digest.into();
    report["graphSha256"] = sha(&graph)?.into();
    report["rustdocFormat"] = doc["format_version"].clone();
    report["mutationRejected"] = id;
    let lock = load(&shapes.join("shapes.lock"))?;
    require(
        lock["shapesSha256"] == report["graphSha256"],
        "shape lock mismatch",
    )?;
    let checked = portable().join("stream-array-shapes-v2");
    require(
        fs::read(checked.join("shapes.json"))? == fs::read(&graph)?,
        "committed structural graph drift",
    )?;
    require(
        semantic_lock(lock)? == semantic_lock(load(&checked.join(format!("{host}.lock")))?)?,
        "committed semantic lock drift",
    )?;
    require(
        report == load(&whole().join("shapes/arrays").join(format!("{host}.json")))?,
        "committed structural receipt drift",
    )?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let scratch = tempfile::tempdir_in(parent)?;
    let product = scratch.path().join("product");
    fs::create_dir(&product)?;
    for file in ["shapes.json", "shapes.lock"] {
        fs::copy(shapes.join(file), product.join(file))?;
    }
    write(&product.join("receipt.json"), report)?;
    renameat_with(CWD, &product, CWD, output, RenameFlags::NOREPLACE)?;
    Ok(())
}
fn main() -> Result<()> {
    match Args::parse().action {
        Action::Replay {
            original,
            framework,
            output,
        } => replay(
            &original.canonicalize()?,
            &framework.canonicalize()?,
            &output,
        ),
        Action::Compare {
            previous,
            current,
            rustdoc,
            receipt,
        } => {
            let mut report =
                compare::compare(&load(&previous)?, &load(&current)?, &load(&rustdoc)?)?;
            report["graphSha256"] = sha(&current)?.into();
            write(&receipt, report)
        }
    }
}
