//! Recover and project a deliberately changed engine model, then compare actual native callers.
use clap::{Parser, ValueEnum};
use public_data_qualification::engine_caller::{self, Role};
use public_data_qualification::{
    Result, ascii_json,
    assembly::{self, ORIGINAL},
    cli_projection::input_bytes,
    json_bytes, list, load, pin, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Copy, Clone, ValueEnum)]
enum Kind {
    Records,
    Errors,
}
struct Case {
    caller: Role,
    lane: &'static str,
    package: &'static str,
    selector: &'static str,
    recovery: &'static str,
    evidence: &'static str,
    edits: &'static [(&'static str, &'static str)],
    observations: &'static [&'static str],
}
impl Kind {
    fn case(self) -> Case {
        match self {
            Self::Records => Case {
                caller: Role::Records,
                lane: "engine-native-records",
                package: "engine-observational-records-package.json",
                selector: "engine-selection.json",
                recovery: "public-root-records",
                evidence: "recovery.lock",
                edits: &[(
                    "    pub records_removed: u32,\n    pub spool_bytes_removed: u64,",
                    "    pub spool_bytes_removed: u64,\n    pub records_removed: u32,",
                )],
                observations: &[
                    "MaintenanceReport {\n    spool_bytes_removed: 0,\n    records_removed: 0,",
                ],
            },
            Self::Errors => Case {
                caller: Role::Errors,
                lane: "engine-public-error",
                package: "engine-public-error-package.json",
                selector: "selection.json",
                recovery: "finite-error-domain",
                evidence: "observation.json",
                edits: &[
                    (
                        "    PathSearchExhausted,\n    ExecutableMissing,",
                        "    ExecutableMissing,\n    PathSearchExhausted,",
                    ),
                    ("#[error(\"engine busy\")]", "#[error(\"engine occupied\")]"),
                    (
                        "        next_offset: u64,\n        available_credit: u32,",
                        "        available_credit: u32,\n        next_offset: u64,",
                    ),
                ],
                observations: &[
                    "reason|PathSearchExhausted|1",
                    "reason|ExecutableMissing|0",
                    "error|Busy|engine occupied|",
                    "InputBackpressure { available_credit: 0, next_offset: 0 }",
                ],
            },
        }
    }
}
#[derive(Parser)]
struct Args {
    #[arg(value_enum)]
    kind: Kind,
    compiler: PathBuf,
    original: PathBuf,
    mutant_target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
}

// Historical locks hash the input bytes including the single spaces after each JSON separator.
fn contract_json(v: &Value) -> Result<String> {
    match v {
        Value::Object(obj) => Ok(format!(
            "{{{}}}",
            obj.iter()
                .map(|(k, v)| Ok(format!(
                    "{}: {}",
                    serde_json::to_string(k)?,
                    contract_json(v)?
                )))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Value::Array(list) => Ok(format!(
            "[{}]",
            list.iter()
                .map(contract_json)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        other => Ok(serde_json::to_string(other)?),
    }
}

fn libraries(project: &Path, target: &Path) -> Result<Vec<Value>> {
    let out = stdout(
        assembly::build_env(&mut Command::new("cargo"))
            .current_dir(project)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--offline",
                "--locked",
                "--lib",
                "-p",
                "marmish-engine",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target),
    )?;
    out.lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}
fn artifact(rows: &[Value], name: &str) -> Result<PathBuf> {
    let mut found = Vec::new();
    for row in rows
        .iter()
        .filter(|r| r["reason"] == "compiler-artifact" && r["target"]["name"] == name)
    {
        for file in list(&row["filenames"])? {
            let path = Path::new(text(file)?);
            if path.extension().is_some_and(|e| e == "rlib") {
                found.push(path.to_owned());
            }
        }
    }
    require(
        found.len() == 1,
        format!("expected one {name} rlib, got {found:?}"),
    )?;
    Ok(found.remove(0))
}
fn stage(case: &Case, inputs: &Path, recovered: &Path) -> Result<PathBuf> {
    fs::create_dir(inputs)?;
    let base = portable().canonicalize()?;
    let mut spec = load(&base.join(case.package))?;
    let copy = |desc: &Value| -> Result {
        let (path, bytes) = input_bytes(&base, desc)?;
        let relative = path.strip_prefix(&base)?;
        require(
            relative == Path::new(text(&desc["path"])?),
            "noncanonical descriptor path",
        )?;
        let dest = inputs.join(relative);
        fs::create_dir_all(dest.parent().ok_or("missing parent")?)?;
        fs::write(dest, bytes)?;
        Ok(())
    };
    copy(&spec["cargoLock"])?;
    let parts = spec["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    let (changed, unchanged) = parts.split_last_mut().ok_or("empty package")?;
    for component in unchanged {
        for field in ["graph", "overlay"] {
            copy(&component[field])?;
        }
    }
    for (role, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
        fs::copy(recovered.join(file), inputs.join(file))?;
        changed[role] = pin(inputs, file)?;
    }
    let path = inputs.join("package.json");
    fs::write(&path, ascii_json(&contract_json(&spec)?))?;
    Ok(path)
}
fn main() -> Result {
    let a = Args::parse();
    let case = a.kind.case();
    let original = a.original.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    let compiler = a.compiler.canonicalize()?;
    let target = std::path::absolute(a.mutant_target)?;
    let scratch = tempfile::tempdir()?;
    let w = scratch.path();
    let checkout = w.join("original");
    run(Command::new("git")
        .args(["clone", "--quiet", "--no-checkout", "--no-hardlinks"])
        .arg(&original)
        .arg(&checkout))?;
    run(Command::new("git")
        .current_dir(&checkout)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    let src = checkout.join("crates/marmish-engine/src/model.rs");
    let mut contents = fs::read_to_string(&src)?;
    let lane = whole().join(case.lane);
    let mut selector = load(&lane.join(case.selector))?;
    require(
        selector["sourceSha256"] == sha(&src)?,
        "unexpected original engine model",
    )?;
    for &(from, to) in case.edits {
        require(
            contents.matches(from).count() == 1,
            "ambiguous source mutation",
        )?;
        contents = contents.replacen(from, to, 1);
    }
    fs::write(&src, contents)?;
    selector["sourceSha256"] = json!(sha(&src)?);
    let selection = w.join("selection.json");
    fs::write(&selection, ascii_json(&contract_json(&selector)?))?;
    let recovered = w.join("recovered");
    run(Command::new(&compiler)
        .args(["recover", case.recovery])
        .arg(src)
        .arg(selection)
        .arg(&recovered))?;
    let reference = libraries(&checkout, &target)?;
    let serde = artifact(&reference, "serde")?;
    let engine = artifact(&reference, "marmish_engine")?;
    let expected = engine_caller::observe(
        case.caller,
        &engine,
        &engine
            .parent()
            .ok_or("missing engine library parent")?
            .join("deps"),
        &serde,
        &w.join("reference"),
    )?;
    let package = stage(&case, &w.join("inputs"), &recovered)?;
    let product = w.join("product");
    run(Command::new(&compiler)
        .args(["build", "native-package", "rust"])
        .arg(package)
        .arg(&product))?;
    let projected = libraries(&product, &w.join("product-target"))?;
    let engine = artifact(&projected, "marmish_engine")?;
    let actual = engine_caller::observe(
        case.caller,
        &engine,
        &engine
            .parent()
            .ok_or("missing engine library parent")?
            .join("deps"),
        &serde,
        &w.join("generated"),
    )?;
    require(
        actual == expected && case.observations.iter().all(|s| actual.0.contains(s)),
        "native mutation observations differ",
    )?;
    let observation = load(&recovered.join(case.evidence))?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let rustc = version.lines().collect::<Vec<_>>();
    let lock = sha(&product.join("package.lock"))?;
    let compiler_sha = sha(&compiler)?;
    // Keep each published receipt's field order for consumers that archive the bytes.
    let receipt = match a.kind {
        Kind::Records => json!({
            "sourceSelection": observation, "output": actual.0,
            "negativeRustDiagnostics": actual.1, "generatedPackageLockSha256": lock,
            "framework": a.framework_revision, "original": ORIGINAL,
            "rustc": rustc, "compilerSha256": compiler_sha
        }),
        Kind::Errors => json!({
            "original": ORIGINAL, "recovery": observation, "output": actual.0,
            "negativeRustDiagnostics": actual.1, "generatedPackageLockSha256": lock,
            "framework": a.framework_revision, "compilerSha256": compiler_sha, "rustc": rustc
        }),
    };
    let destination = std::path::absolute(&a.receipt)?;
    let mut file =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("missing receipt parent")?)?;
    file.write_all(&json_bytes(&receipt)?)?;
    file.persist(&a.receipt)?;
    Ok(())
}
