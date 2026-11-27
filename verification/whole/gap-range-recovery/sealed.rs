//! Build both projections before staging either independent caller in the sandbox.
use super::{LEGACY, MODEL, PACKAGE, stage};
use public_data_qualification::{
    Result,
    assembly::{self, Sandbox},
    capture, load, portable,
    product::pinned,
    require, run, sha, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) struct Inputs {
    pub(super) compiler: PathBuf,
    pub(super) framework: PathBuf,
    pub(super) original: PathBuf,
    pub(super) scratch: PathBuf,
    pub(super) baseline_doc: PathBuf,
    pub(super) mutation_doc: PathBuf,
    pub(super) host: String,
}

fn generated_sources(product: &Path, label: &str) -> Result<Value> {
    let lock = load(&product.join("package.lock"))?;
    let files = lock["generatedFiles"]
        .as_object()
        .ok_or("missing generated files")?;
    for (name, digest) in files {
        let path = Path::new(name);
        require(
            !name.is_empty()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "invalid generated source path",
        )?;
        require(
            *digest == sha(&product.join(path))?,
            "generated Rust differs from package lock",
        )?;
    }
    if label == "baseline" {
        let authored = portable().join("engine-gap-range-v1/package.lock");
        pinned(&authored, LEGACY)?;
        require(
            lock["generatedFiles"] == load(&authored)?["generatedFiles"],
            "recovered baseline changed generated Rust",
        )?;
    }
    Ok(lock["generatedFiles"].clone())
}

fn observe(sandbox: &Sandbox, lane: &Path, expected: &Value, label: &str) -> Result<Value> {
    let caller = lane.join("caller");
    fs::create_dir_all(caller.join("src"))?;
    let reference = whole().join("gap-range-reference");
    for (source, target) in [
        ("caller.toml", "Cargo.toml"),
        ("caller.lock", "Cargo.lock"),
        ("probe.rs", "src/main.rs"),
    ] {
        fs::copy(reference.join(source), caller.join(target))?;
    }
    run(sandbox.command(&caller)?.args([
        "cargo",
        "rustc",
        "--locked",
        "--offline",
        "--release",
        "--",
        "--cfg",
        "with_range",
    ]))?;
    let exe = caller.join("target/release/gap-observer");
    let observed = capture(
        sandbox.command(&caller)?.arg(&exe),
        600,
        Some(&fs::read(reference.join("cases.json"))?),
    )?;
    require(
        observed.status.success(),
        format!(
            "{label} caller failed: {}",
            String::from_utf8_lossy(&observed.stderr)
        ),
    )?;
    let rows: Value = serde_json::from_slice(&observed.stdout)?;
    require(
        rows == *expected,
        format!("recovered {label} data differs from independent caller evidence"),
    )?;
    fs::copy(
        whole().join("gap-range-record/traits.rs"),
        caller.join("src/main.rs"),
    )?;
    run(sandbox
        .command(&caller)?
        .args(["cargo", "build", "--locked", "--offline", "--release"]))?;
    require(
        run(sandbox.command(&caller)?.arg(&exe))?
            == b"public fields, reversed interval, Clone and Eq\n",
        "native public traits differ",
    )?;
    Ok(rows)
}

pub(super) fn check(args: Inputs, expected: &Value) -> Result<Value> {
    let isolated = tempfile::tempdir()?;
    let work = isolated.path().canonicalize()?;
    let compiler = work.join("oddlang");
    fs::copy(&args.compiler, &compiler)?;
    let model = args.original.join(MODEL);
    let mutation = args.scratch.join("original-mutant").join(MODEL);
    let probes = [
        model.with_file_name("lib.rs"),
        model,
        mutation.with_file_name("lib.rs"),
        mutation,
        args.baseline_doc,
        args.mutation_doc,
        whole().join("gap-range-reference/probe.rs"),
        whole().join("gap-range-record/traits.rs"),
    ];
    let mut denied = vec![
        assembly::root(),
        args.original.as_path(),
        &args.framework,
        args.compiler.parent().ok_or("missing compiler parent")?,
        &args.scratch,
    ];
    denied.extend(probes.iter().map(PathBuf::as_path));
    let cache = work.join("empty-git-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(&args.host, &work, &denied, &cache)?;
    sandbox.deny(&probes)?;
    for label in ["baseline", "mutation"] {
        let lane = work.join(label);
        let inputs = lane.join("inputs");
        fs::create_dir_all(&inputs)?;
        stage(&args.scratch.join(label).join("recovered"), &inputs)?;
        run(sandbox
            .command(&lane)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(inputs.join(PACKAGE))
            .arg(lane.join("product")))?;
    }
    let mut observations = json!({});
    for label in ["baseline", "mutation"] {
        let lane = work.join(label);
        let manifest = lane.join("inputs").join(PACKAGE);
        let product = lane.join("product");
        let lock = product.join("package.lock");
        let sources = generated_sources(&product, label)?;
        let rows = observe(&sandbox, &lane, &expected[label], label)?;
        observations[label] = json!({"observations":rows,"manifestSha256":sha(&manifest)?,
            "packageLockSha256":sha(&lock)?,"generatedFiles":sources});
        let recovered = args.scratch.join(label).join("recovered");
        fs::copy(manifest, recovered.join("package.json"))?;
        fs::copy(lock, recovered.join("package.lock"))?;
    }
    Ok(json!({"cases":observations,"scopeSourceDenials":probes.len()}))
}
