//! Recover the Launch release, project it in isolation and falsify its guard order.
mod recover;
use clap::Parser;
use copy_checkpoint_replay::{build_compiler, compare, consumer, observe, stage};
use copy_manifest_replay::launch_data;
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, Sandbox, bytes_sha, checkout, root},
    json_bytes, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "5f4cf958b7b4f8da8f8309f87734c14a8309c7d7";
#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, FRAMEWORK, false)?;
    let temp = tempfile::tempdir()?;
    let private = temp.path().canonicalize()?;
    let build = tempfile::tempdir()?;
    let work = build.path().canonicalize()?;
    let cli = build_compiler(&framework, &private.join("tools"), "+1.97.1")?;
    let caller = whole().join("copy-launch/caller.rs");
    let old = consumer(
        &original.join("crates/marmish-stream"),
        &private.join("reference"),
        &original.join("Cargo.lock"),
        &caller,
    )?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let enums = [json!({"operation":"enums"})];
    let table: Value = serde_json::from_slice(&observe(&old, &enums)?)?;
    recover::derive(&original, &cli, &table, host, &private.join("recovered"))?;
    let (plan, mut manifest) = stage(&work.join("inputs"), "stream-launch-package.json", false)?;
    let inputs = plan
        .parent()
        .ok_or("missing input directory")?
        .join("copy-launch-v1");
    for name in [
        "launch.json",
        "launch-rust.json",
        "enums.json",
        "enums-rust.json",
    ] {
        fs::copy(private.join("recovered").join(name), inputs.join(name))?;
    }
    let compiler = work.join("oddlang");
    fs::copy(cli, &compiler)?;
    let cache = work.join("cache");
    fs::create_dir(&cache)?;
    let companion = root().canonicalize()?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&original, &framework, &companion, &private],
        &cache,
    )?;
    let project = |name: &str| -> Result<PathBuf> {
        let dest = work.join(name);
        sandbox.deny(&[caller.clone(), original.join("Cargo.toml")])?;
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&dest))?;
        run(sandbox.command(&dest)?.args([
            "cargo",
            "build",
            "--locked",
            "--offline",
            "--release",
        ]))?;
        Ok(dest)
    };
    let package = project("generated")?;
    let lock = package.join("package.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join("copy-launch-v1/package.lock"))?,
        "package lock differs",
    )?;
    let observer = |dir: &Path, name: &str| {
        consumer(dir, &private.join(name), &dir.join("Cargo.lock"), &caller)
    };
    let new = observer(&package, "native")?;
    compare("enum tags", &new, &enums, std::slice::from_ref(&table))?;
    let (creates, expected) = launch_data::constructs();
    for (name, exe) in [("original", &old), ("generated", &new)] {
        compare(name, exe, &creates, &expected)?;
    }
    let decodes = launch_data::decodes();
    let raw = observe(&old, &decodes)?;
    let decoded = std::str::from_utf8(&raw)?
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<Vec<Value>>>()?;
    compare("decodes", &new, &decodes, &decoded)?;
    let pin = &mut manifest["components"]
        .as_array_mut()
        .and_then(|c| c.last_mut())
        .ok_or("missing Launch component")?["graph"];
    let path = plan.parent().unwrap().join(text(&pin["path"])?);
    let mut graph = load(&path)?;
    let guards = graph["guards"].as_array_mut().ok_or("missing guards")?;
    require(
        guards
            .first()
            .is_some_and(|g| g["id"] == "urn:marmish:copy:launch:guard:version"),
        "first guard must check version",
    )?;
    guards.rotate_left(1);
    fs::write(&path, json_bytes(&graph)?)?;
    pin["sha256"] = json!(sha(&path)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let mutant = observer(&project("mutant")?, "reordered")?;
    let (req, baseline) = creates
        .iter()
        .zip(&expected)
        .find(|(r, _)| r["version"] == 0 && r["source"] == json!([]))
        .ok_or("missing guard priority case")?;
    require(
        baseline["valid"]["Err"][0] == "Invalid(\"copy_version\")",
        "baseline did not prioritize version",
    )?;
    let mut altered = baseline.clone();
    altered["valid"] = json!({"Err":["Invalid(\"copy_path\")","invalid stream input: copy_path"]});
    compare(
        "reordered guards",
        &mutant,
        std::slice::from_ref(req),
        &[altered],
    )?;
    let outcomes = table["enums"].as_object().ok_or("missing enums")?.len() * 256;
    let receipt = json!({"original":ORIGINAL,"framework":FRAMEWORK,"rust":rust.trim(),"originalAndCompanionReadDenied":true,"packageLockSha256":sha(&lock)?,"callerSha256":sha(&caller)?,"nativeEnumByteOutcomes":outcomes,"constructCases":creates.len(),"exactDecodeCases":decodes.len(),"negativeControl":"version guard moved last selects copy_path instead of copy_version","observationsSha256":bytes_sha(&serde_json::to_vec(&json!([table,expected,decoded]))?)});
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
