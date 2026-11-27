//! Compare a recovered schema, projection and 2021 native consumer with the locked upstream.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::ORIGINAL, load, portable, require, run, sha, whole,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use stream_wire_replay::{check_original, compare_caller, original_tree, write_receipt};
#[derive(Parser)]
struct Args {
    oddlang: PathBuf,
    original: PathBuf,
    receipt: PathBuf,
}
fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let rust = check_original(&original)?;
    let contract = portable().join("stream-wire-schema-v1");
    let tmp = tempfile::tempdir()?;
    let work = tmp.path();
    let observed = work.join("observed");
    let generated = work.join("generated");
    let source = original.join("crates/marmish-stream/src/wire.rs");
    run(Command::new(&args.oddlang)
        .args(["recover", "wire-schema"])
        .arg(&source)
        .arg(contract.join("selection.json"))
        .arg(&observed))?;
    for file in ["graph.json", "rust.json", "observation.json"] {
        require(
            fs::read(observed.join(file))? == fs::read(contract.join(file))?,
            format!("recovered artifact differs: {file}"),
        )?;
    }
    run(Command::new(&args.oddlang)
        .args(["build", "wire-schema", "rust"])
        .arg(contract.join("graph.json"))
        .arg(contract.join("rust.json"))
        .arg(&generated))?;
    for (actual, pin) in [
        ("projection.lock", "rust.lock"),
        ("obligations.json", "obligations.json"),
    ] {
        require(
            fs::read(generated.join(actual))? == fs::read(contract.join(pin))?,
            format!("projected artifact differs: {pin}"),
        )?;
    }
    let caller = whole().join("stream-wire/caller.rs");
    let local = work.join("caller.rs");
    fs::copy(&caller, &local)?;
    fs::copy(generated.join("wire.rs"), work.join("projected.rs"))?;
    let bytes = compare_caller(
        &original,
        &work.join("cargo"),
        work,
        &local,
        &["marmish_stream", "serde", "serde_json", "base64"],
        "2021",
        None,
    )?;
    let mut results: Value = serde_json::from_slice(&bytes)?;
    write_receipt(
        &args.receipt,
        json!({"originalRevision":ORIGINAL,"originalTree":original_tree(&original)?,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,
        "sourceSha256":sha(&source)?,"rustc":rust,"projection":load(&generated.join("projection.lock"))?,"callerSha256":sha(&caller)?,"results":results}),
    )?;
    results.sort_all_objects();
    let fields = results
        .as_object()
        .ok_or("expected a caller summary")?
        .iter()
        .map(|(key, value)| Ok(format!("{}: {value}", serde_json::to_string(key)?)))
        .collect::<Result<Vec<_>>>()?;
    println!("{{{}}}", fields.join(", "));
    Ok(())
}
