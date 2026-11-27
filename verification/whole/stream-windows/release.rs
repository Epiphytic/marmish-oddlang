//! Generate the windows package with original sources denied, then qualify its native behavior.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{self, Sandbox, bytes_sha},
    json_bytes, list, load, portable, require, run, sha, stdout, text,
};
use serde_json::json;
use std::{fs, io::Write, path::PathBuf, process::Command};
use stream_wire_replay::{native, window};
#[derive(Parser)]
struct Args {
    original: PathBuf,
    oddlang: PathBuf,
    receipt: PathBuf,
}
fn tests(output: &str) -> Vec<&str> {
    let mut names: Vec<_> = output
        .lines()
        .filter_map(|l| {
            let n = l.strip_prefix("test windows::")?.strip_suffix(" ... ok")?;
            let (_, name) = n.split_once("tests::")?;
            Some(name)
        })
        .collect();
    names.sort();
    names
}
fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    let root = assembly::root().canonicalize()?;
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let inputs = work.join("inputs");
    let (plan, mut manifest) = native::stage(&inputs, "stream-windows-package.json")?;
    let compiler = work.join("oddlang");
    fs::copy(a.oddlang, &compiler)?;
    let cache = work.join("empty-provider-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(assembly::DARWIN, &work, &[&original, &root], &cache)?;
    let make = |name: &str| -> Result<PathBuf> {
        let dest = work.join(name);
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&dest))?;
        Ok(dest)
    };
    let product = make("generated")?;
    let lock = product.join("package.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join("stream-windows-v1/package.lock"))?,
        "generated package lock differs",
    )?;
    for (file, pin) in load(&lock)?["generatedFiles"]
        .as_object()
        .ok_or("missing generated files")?
    {
        require(
            pin == &sha(&product.join(file))?,
            format!("invalid generated file {file}"),
        )?;
    }
    let target = work.join("native-build");
    run(sandbox
        .command(&product)?
        .args([
            "cargo",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&target))?;
    let source = original.join("crates/marmish-stream/src/windows.rs");
    sandbox.deny(std::slice::from_ref(&source))?;
    let mut receipt = window::compare(&original, &product)?;
    let source = fs::read_to_string(source)?;
    let suite = native::test_module(&source)?;
    let components = list(&manifest["components"])?;
    let which = components
        .iter()
        .position(|c| c["kind"] == "sequence-journal")
        .ok_or("missing sequence journal")?;
    let group = components
        .iter()
        .position(|c| c["module"] == "windows")
        .ok_or("missing windows module")?;
    let leaf = product.join(format!("src/group_{group}/__projection_{which}.rs"));
    let before = sha(&leaf)?;
    let mut file = fs::OpenOptions::new().append(true).open(&leaf)?;
    file.write_all(b"\n#[cfg(test)]\nuse crate::{Error, MAX_BLOCK, MAX_RECORDS, WINDOW_BYTES};\n")?;
    file.write_all(suite.as_bytes())?;
    let base = stdout(
        Command::new("cargo")
            .current_dir(&original)
            .args([
                "+1.97.1",
                "test",
                "--release",
                "--locked",
                "--offline",
                "-p",
                "marmish-stream",
                "--target-dir",
            ])
            .arg(work.join("original-tests"))
            .args(["--lib", "windows::tests"]),
    )?;
    let other = stdout(
        sandbox
            .command(&product)?
            .args([
                "cargo",
                "test",
                "--release",
                "--locked",
                "--offline",
                "--target-dir",
            ])
            .arg(&target)
            .args(["--lib", "windows::"]),
    )?;
    let names = tests(&base);
    require(
        names.len() == 5 && names == tests(&other),
        "original suite differs",
    )?;
    receipt["lexicalTests"] = json!(names);
    receipt["windowTestModuleSha256"] = json!(bytes_sha(suite.as_bytes()));
    receipt["generatedModuleBeforeTestsSha256"] = json!(before);
    let pin = &mut manifest["components"][which]["graph"];
    let policy = inputs.join(text(&pin["path"])?);
    let mut graph = load(&policy)?;
    let mut changed = 0;
    for op in graph["operations"]
        .as_array_mut()
        .ok_or("missing operations")?
    {
        for c in op["cases"].as_array_mut().ok_or("missing cases")? {
            if c["reject"]
                .as_str()
                .is_some_and(|s| s.ends_with(":Conflict"))
            {
                c["reject"] = json!("urn:marmish:stream:error:OldRetry");
                changed += 1;
            }
        }
    }
    require(changed == 1, "expected one conflict rule")?;
    fs::write(&policy, serde_json::to_vec(&graph)?)?;
    pin["sha256"] = json!(sha(&policy)?);
    fs::write(&plan, serde_json::to_vec(&manifest)?)?;
    let mutant = make("mutant")?;
    let err = window::compare(&original, &mutant)
        .err()
        .ok_or("changed retained-conflict rule passed")?;
    require(
        err.to_string()
            .contains("exact_retry_conflict_gap_eof_and_partial_delivery"),
        format!("unexpected mutant error: {err}"),
    )?;
    receipt["mutation"] = json!(
        "changing the retained conflict to OldRetry fails the unchanged upstream public test"
    );
    receipt["isolation"] =
        json!("Seatbelt read allowlist and network deny during generation and release build");
    fs::write(a.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
