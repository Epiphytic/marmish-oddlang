//! Recover the selected Rust enum and project its sealed native package in one fresh workspace.
use clap::Parser;
use public_data_qualification::{Result, json_bytes, load, portable, require, run, sha, text};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    compiler: PathBuf,
    source: PathBuf,
    selection: PathBuf,
    output: PathBuf,
}

fn project(compiler: &Path, recovered: &Path, work: &Path) -> Result<Value> {
    let inputs = work.join("portable-inputs");
    fs::create_dir(&inputs)?;
    let mut spec = load(&portable().join("engine-gap-reason-package.json"))?;
    for (key, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
        let output = inputs.join(file);
        fs::copy(recovered.join(file), &output)?;
        let pin = &mut spec["components"][0][key];
        pin["path"] = json!(file);
        pin["sha256"] = json!(sha(&output)?);
    }
    let lock = inputs.join("Cargo.lock");
    fs::copy(portable().join(text(&spec["cargoLock"]["path"])?), &lock)?;
    require(
        spec["cargoLock"]["sha256"] == sha(&lock)?,
        "wrong provider lock",
    )?;
    spec["cargoLock"]["path"] = json!("Cargo.lock");
    let manifest = inputs.join("package.json");
    fs::write(&manifest, json_bytes(&spec)?)?;
    let product = work.join("product");
    run(Command::new(compiler)
        .args(["build", "native-package", "rust"])
        .arg(&manifest)
        .arg(&product)
        .current_dir(work))?;
    let lock = product.join("package.lock");
    let package = load(&lock)?;
    for (name, digest) in package["generatedFiles"]
        .as_object()
        .ok_or("missing generated files")?
    {
        let path = product.join(name).canonicalize()?;
        require(
            path.starts_with(&product) && *digest == sha(&path)?,
            "generated file differs from package lock",
        )?;
    }
    Ok(
        json!({"manifestSha256":sha(&manifest)?, "packageLockSha256":sha(&lock)?, "generatedFiles":package["generatedFiles"],"projection":package["components"][0]["projection"]}),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let compiler = args.compiler.canonicalize()?;
    let source = args.source.canonicalize()?;
    let selection = args.selection.canonicalize()?;
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let stage = tempfile::tempdir_in(parent)?;
    let recovered = stage.path().join("recovered");
    run(Command::new(&compiler)
        .args(["recover", "serde-string-choice"])
        .arg(&source)
        .arg(&selection)
        .arg(&recovered))?;
    let lock = load(&recovered.join("recovery.lock"))?;
    for (field, file) in [
        ("source", source),
        ("selection", selection),
        ("graph", recovered.join("graph.json")),
        ("overlay", recovered.join("rust.json")),
    ] {
        require(
            lock[format!("{field}Sha256")] == sha(&file)?,
            format!("wrong recovery digest: {field}"),
        )?;
    }
    let candidate = project(&compiler, &recovered, stage.path())?;
    renameat_with(CWD, stage.path(), CWD, &args.output, RenameFlags::NOREPLACE)?;
    println!("{}", json!({"recovery":lock,"candidate":candidate}));
    Ok(())
}
