pub mod corpus;
pub mod manifest;
use copy_checkpoint_replay::{build_compiler, compare, consumer, stage};
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox, bytes_sha},
    json_bytes, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub struct Cases {
    pub requests: Vec<Value>,
    pub expected: Vec<Value>,
    pub metadata: Value,
    pub mutant: Vec<(Value, Value)>,
    pub negative: &'static str,
}
pub fn workspace_cases(
    original: &Path,
    generated: &Path,
    work: &Path,
    caller: &Path,
    c: &Cases,
) -> Result<Value> {
    for (label, root, lock) in [
        (
            "original",
            original.join("crates/marmish-stream"),
            original.join("Cargo.lock"),
        ),
        (
            "generated",
            generated.join("marmish-stream"),
            generated.join("Cargo.lock"),
        ),
    ] {
        let exe = consumer(
            &root,
            &work.join(format!("relation-{label}")),
            &lock,
            caller,
        )?;
        compare(label, &exe, &c.requests, &c.expected)?;
    }
    Ok(
        json!({"cases":c.requests.len(),"callerSha256":sha(caller)?,"observationsSha256":bytes_sha(&serde_json::to_vec(&c.expected)?)}),
    )
}
pub fn verify(
    name: &str,
    pin: &str,
    original: &Path,
    framework: &Path,
    receipt: &Path,
    cases: Cases,
    mutate: fn(&mut Value) -> Result,
) -> Result {
    let original = original.canonicalize()?;
    let framework = framework.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    assembly::checkout(&framework, pin, false)?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let private = tempfile::tempdir()?;
    let cli = build_compiler(&framework, &private.path().join("tools"), "+1.97.1")?;
    let compiler = work.join("oddlang");
    fs::copy(cli, &compiler)?;
    let (plan, mut manifest) = stage(
        &work.join("inputs"),
        &format!("stream-{name}-package.json"),
        false,
    )?;
    let caller = whole().join(name).join("caller.rs");
    let root = assembly::root().canonicalize()?;
    let cache = work.join("empty-provider-cache");
    fs::create_dir(&cache)?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("host missing")?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&root, &original, &framework, private.path()],
        &cache,
    )?;
    let project = |label: &str| -> Result<PathBuf> {
        let dir = work.join(label);
        sandbox.deny(&[caller.clone(), original.join("Cargo.toml")])?;
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&dir))?;
        run(sandbox
            .command(&dir)?
            .args(["cargo", "build", "--locked", "--offline", "--release"]))?;
        Ok(dir)
    };
    let product = project("native")?;
    let lock = product.join("package.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join(format!("{name}-v1/package.lock")))?,
        "archived package lock differs",
    )?;
    for (label, root, lock) in [
        (
            "original",
            original.join("crates/marmish-stream"),
            original.join("Cargo.lock"),
        ),
        ("generated", product.clone(), product.join("Cargo.lock")),
    ] {
        let exe = consumer(&root, &private.path().join(label), &lock, &caller)?;
        compare(label, &exe, &cases.requests, &cases.expected)?;
    }
    let graph = &mut manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?
        .last_mut()
        .ok_or("empty components")?["graph"];
    let path = plan.parent().unwrap().join(text(&graph["path"])?);
    let mut data = load(&path)?;
    mutate(&mut data)?;
    fs::write(&path, json_bytes(&data)?)?;
    graph["sha256"] = json!(sha(&path)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let mutant = project("mutant")?;
    let exe = consumer(
        &mutant,
        &private.path().join("mutated"),
        &mutant.join("Cargo.lock"),
        &caller,
    )?;
    let (req, want): (Vec<_>, Vec<_>) = cases.mutant.into_iter().unzip();
    compare("mutant", &exe, &req, &want)?;
    let mut value = json!({"original":ORIGINAL,"framework":pin,"rust":rust.trim(),"originalAndCompanionReadDenied":true,"packageLockSha256":sha(&lock)?,"callerSha256":sha(&caller)?});
    value
        .as_object_mut()
        .unwrap()
        .extend(cases.metadata.as_object().unwrap().clone());
    value["observationsSha256"] = json!(bytes_sha(&serde_json::to_vec(&cases.expected)?));
    value["negativeControl"] = json!(cases.negative);
    fs::write(receipt, json_bytes(&value)?)?;
    println!("Qualified {name}: {}", cases.metadata);
    Ok(())
}
