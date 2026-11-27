mod cases;
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, bytes_sha},
    load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};
use stream_wire_replay::{native, write_receipt};
#[derive(Parser)]
struct Args {
    original: PathBuf,
    oddlang: PathBuf,
    recovery: PathBuf,
    receipt: PathBuf,
}
const TESTS: [&str; 5] = [
    "roundtrips_and_fallback",
    "incompressible_maximum_block_uses_none",
    "rejects_corruption_truncation_frames_and_dictionary",
    "rejects_nonbeneficial_zlib_representation",
    "rejects_bounds_and_length",
];
fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    let inputs = work.join("inputs");
    let (plan, mut manifest) = native::stage(&inputs, "stream-block-package.json")?;
    let build = |dir: &std::path::Path| {
        run(Command::new(&a.oddlang)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(dir))
    };
    let projected = work.join("projected");
    build(&projected)?;
    require(
        fs::read(projected.join("package.lock"))?
            == fs::read(portable().join("stream-block-package-v1/package.lock"))?,
        "regenerated package lock differs",
    )?;
    run(Command::new("cargo")
        .env_remove("CARGO_TARGET_DIR")
        .current_dir(&projected)
        .args(["+1.97.1", "build", "--offline", "--locked", "--release"]))?;
    let contract = portable().join("stream-shared-v1");
    let recovered = work.join("observed");
    run(Command::new(a.recovery)
        .args(["recover", "shared-declarations"])
        .arg(&original)
        .arg(contract.join("selection.json"))
        .arg(&recovered))?;
    for file in ["graph.json", "rust.json", "observation.json"] {
        require(
            fs::read(recovered.join(file))? == fs::read(contract.join(file))?,
            format!("recovery differs: {file}"),
        )?;
    }
    let source = stdout(Command::new("git").current_dir(&original).args([
        "show",
        &format!("{ORIGINAL}:crates/marmish-stream/src/codec.rs"),
    ]))?;
    let suite = native::test_module(&source)?;
    let tests = format!(
        "use marmish_stream::{{Error, MAX_BLOCK, MAX_OFFSET}};\nuse marmish_stream::codec::*;\n{suite}"
    );
    let names: Vec<_> = TESTS.iter().map(|t| format!("tests::{t}")).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    let main = whole().join("stream-block/caller.rs");
    let compile = |root: &std::path::Path, lock: &std::path::Path, name: &str| {
        native::caller(
            root,
            &work.join(name),
            lock,
            &main,
            tests.as_bytes(),
            &names,
        )
    };
    let left = compile(
        &original.join("crates/marmish-stream"),
        &original.join("Cargo.lock"),
        "original",
    )?;
    let right = compile(&projected, &projected.join("Cargo.lock"), "candidate")?;
    let mut result = cases::qualify(&left, &right)?;
    let pin = &mut manifest["components"][1]["graph"];
    let path = inputs.join(text(&pin["path"])?);
    let mut graph = load(&path)?;
    let maximum = graph["bounds"]["offsetMaximum"]
        .as_u64()
        .ok_or("invalid offsetMaximum")?
        .checked_sub(1)
        .ok_or("cannot reduce offset bound")?;
    graph["bounds"]["offsetMaximum"] = json!(maximum);
    fs::write(&path, serde_json::to_vec(&graph)?)?;
    pin["sha256"] = json!(sha(&path)?);
    fs::write(&plan, serde_json::to_vec(&manifest)?)?;
    let mutant = work.join("changed");
    build(&mutant)?;
    let changed = compile(&mutant, &mutant.join("Cargo.lock"), "mutant")?;
    let case = json!({"last-byte":{"operation":"encode","negotiated":"none","offset":i64::MAX-1,"bytes":[0]}});
    let base = cases::send(&left, &case)?;
    let altered = cases::send(&changed, &case)?;
    require(
        base["last-byte"].get("wire").is_some() && altered["last-byte"]["error"] == "Offset",
        "offset mutation did not distinguish last valid byte",
    )?;
    result["mutation"] = json!({"offsetMaximum":maximum,"error":altered["last-byte"]});
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let more = json!({"reference":ORIGINAL,"sourceSuiteSha256":bytes_sha(suite.as_bytes()),"upstreamTests":TESTS,"nativeCompiler":rust.lines().collect::<Vec<_>>(),"packageLockSha256":sha(&projected.join("package.lock"))?,"publicCallerSha256":sha(&main)?,"manifestSha256":sha(&portable().join("stream-block-package.json"))?});
    result
        .as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    write_receipt(&a.receipt, result)
}
