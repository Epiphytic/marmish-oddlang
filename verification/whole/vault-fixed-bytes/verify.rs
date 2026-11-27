//! Recover the observed owned array extent, project it in isolation and test the public API.
use clap::{Parser, Subcommand};
use copy_checkpoint_replay::{build_compiler, stage};
use public_data_qualification::{
    assembly::{self, ORIGINAL, Sandbox},
    vault_witness as witness, *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FRAMEWORK: &str = "595fb7a55dc2016cfab70c5b131bf2f6acfe9983";
const PACKAGE: &str = "vault-fixed-value-package.json";
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Qualify {
        framework: PathBuf,
        original: PathBuf,
        cache: PathBuf,
        receipt: PathBuf,
        #[arg(long)]
        rust_capture: PathBuf,
    },
    Workspace {
        original: PathBuf,
        product: PathBuf,
        reference_target: PathBuf,
        host: String,
        work: PathBuf,
    },
}

fn one<T>(mut matches: impl Iterator<Item = T>, label: &str) -> Result<T> {
    let item = matches.next().ok_or_else(|| format!("missing {label}"))?;
    require(matches.next().is_none(), format!("ambiguous {label}"))?;
    Ok(item)
}

fn storage(observation: &Value, key: &str, value: &str) -> Result<Value> {
    let declarations = list(&observation["graph"]["declarations"])?;
    let decl = one(
        declarations.iter().filter(|d| d[key] == value),
        "vault declaration",
    )?;
    let field = one(list(&decl["fields"])?.iter(), "storage field")?;
    Ok(field.get("type").ok_or("missing storage type")?.clone())
}

fn library(product: &Path) -> Result<PathBuf> {
    let mut matches = vec![];
    for entry in fs::read_dir(product.join("target/release/deps"))? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.starts_with("libmarmish_vault-") && p.ends_with(".rlib"))
        {
            matches.push(path);
        }
    }
    one(matches.into_iter(), "vault rlib")
}

fn width_control(harness: &Path, mutant: &Path, out: &Path) -> Result<Value> {
    let lib = library(mutant)?;
    let result = capture(
        &mut witness::link_command(harness, &lib, lib.parent().unwrap(), out, true),
        600,
        None,
    )?;
    require(
        !result.status.success(),
        "width mutation passed the original assertion",
    )?;
    let diagnostics = std::str::from_utf8(&result.stderr)?
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(serde_json::from_str)
        .collect::<serde_json::Result<Vec<Value>>>()?;
    let typed = one(
        diagnostics
            .iter()
            .filter(|d| d["level"] == "error" && !d["code"].is_null()),
        "typed compiler error",
    )?;
    let message = text(&typed["message"])?;
    require(
        typed["code"]["code"] == "E0277"
            && message.contains("[u8; 31]")
            && message.contains("32]")
            && text(&typed["rendered"])?.contains("Secret::from_slice(&[7; 32])"),
        "unexpected width diagnostic",
    )?;
    Ok(
        json!({"length":31,"phase":"compile unchanged test module","diagnostic":typed["code"]["code"],"compilerExit":result.status.code()}),
    )
}

fn qualify(framework: &Path, original: &Path, cache: &Path, capture_tool: &Path) -> Result<Value> {
    let original = original.canonicalize()?;
    let framework = framework.canonicalize()?;
    assembly::checkout(&framework, FRAMEWORK, false)?;
    assembly::checkout(&original, ORIGINAL, false)?;
    let capture_tool = capture_tool.canonicalize()?;
    fs::create_dir_all(cache)?;
    let cache = cache.canonicalize()?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    require(
        [assembly::LINUX, "aarch64-apple-darwin"].contains(&host),
        "unsupported build sandbox host",
    )?;
    let private = tempfile::tempdir()?;
    let scratch = private.path().canonicalize()?;
    let stage_dir = tempfile::tempdir()?;
    let work = stage_dir.path().canonicalize()?;
    let compiler = build_compiler(&framework, &cache.join("compiler"), "+1.96.1")?;
    let cli = work.join("oddlang");
    fs::copy(compiler, &cli)?;
    let crate_dir = original.join("crates/marmish-vault");
    let raw = witness::capture_shapes(&capture_tool, &crate_dir, host)?;
    let expected_type = storage(
        &serde_json::from_slice(&raw)?,
        "id",
        "local::marmish_vault::Secret::{type}",
    )?;
    let input = scratch.join("original.json");
    fs::write(&input, raw)?;
    let selection = whole().join("vault-fixed-bytes/selection.json");
    let recovered = scratch.join("recovered");
    run(Command::new(&cli)
        .args(["recover", "fixed-bytes"])
        .arg(&input)
        .arg(&selection)
        .arg(&recovered))?;
    let graph = recovered.join("graph.json");
    require(
        fs::read(&graph)? == fs::read(portable().join("vault-fixed-bytes-v1/graph.json"))?,
        "recovered fixed-width contract differs",
    )?;
    let (plan, mut manifest) = stage(&work.join("inputs"), PACKAGE, false)?;
    let part = one(
        manifest["components"]
            .as_array_mut()
            .ok_or("missing components")?
            .iter_mut()
            .filter(|p| {
                p["reexports"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!("Secret")))
            }),
        "Secret component",
    )?;
    let pin = &mut part["graph"];
    let selected = plan.parent().unwrap().join(text(&pin["path"])?);
    require(
        pin["sha256"] == sha(&graph)?,
        "recovered graph does not match the package pin",
    )?;
    fs::copy(&graph, &selected)?;
    let providers = work.join("providers");
    fs::create_dir(&providers)?;
    let root = assembly::root().canonicalize()?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&original, &root, &framework, &scratch],
        &providers,
    )?;
    let project = |name: &str| -> Result<PathBuf> {
        let dest = work.join(name);
        sandbox.build_native_package(
            &cli,
            &plan,
            &dest,
            &[
                selection.clone(),
                original.join("Cargo.toml"),
                framework.join("Cargo.toml"),
            ],
        )?;
        Ok(dest)
    };
    let package = project("generated")?;
    let projected: Value =
        serde_json::from_slice(&witness::capture_shapes(&capture_tool, &package, host)?)?;
    require(
        expected_type == storage(&projected, "name", "Secret")?,
        "storage provider, element or extent differs",
    )?;
    let lib = library(&package)?;
    let harness = witness::strict(
        &original,
        &cache.join("reference"),
        host,
        &scratch,
        &lib,
        lib.parent().unwrap(),
    )?;
    let mut altered = load(&selected)?;
    let definition = one(
        altered["types"]
            .as_object_mut()
            .ok_or("missing types")?
            .values_mut(),
        "fixed value",
    )?;
    require(definition["length"] == 32, "unexpected baseline width")?;
    definition["length"] = json!(31);
    fs::write(&selected, serde_json::to_vec_pretty(&altered)?)?;
    pin["sha256"] = json!(sha(&selected)?);
    fs::write(&plan, serde_json::to_vec_pretty(&manifest)?)?;
    let width = width_control(&harness, &project("mutant")?, &scratch.join("mutant-test"))?;
    Ok(
        json!({"framework":FRAMEWORK,"original":ORIGINAL,"host":host,"profile":"release","test":witness::STRICT,"unchangedModuleSha256":witness::MODULE,"sourceSha256":sha(&crate_dir.join("src/lib.rs"))?,"graphSha256":sha(&graph)?,"selectedStorage":expected_type,"packageManifestSha256":sha(&portable().join(PACKAGE))?,"packageLockSha256":sha(&package.join("package.lock"))?,"cargoLockSha256":sha(&package.join("Cargo.lock"))?,"sourceReadDenied":true,"originalPassed":1,"generatedPassed":1,"recovery":load(&recovered.join("recovery.lock"))?,"widthControl":width}),
    )
}
fn main() -> Result {
    match Args::parse().command {
        Mode::Qualify {
            framework,
            original,
            cache,
            receipt,
            rust_capture,
        } => fs::write(
            receipt,
            json_bytes(&qualify(&framework, &original, &cache, &rust_capture)?)?,
        )?,
        Mode::Workspace {
            original,
            product,
            reference_target,
            host,
            work,
        } => {
            assembly::checkout(&original, ORIGINAL, false)?;
            let lib = library(&product)?;
            let deps = lib.parent().unwrap();
            witness::strict(&original, &reference_target, &host, &work, &lib, deps)?;
            let hold = witness::holdout(&original, &work, &lib, deps)?;
            println!(
                "{}",
                json!({"name":witness::STRICT,"unchangedModuleSha256":witness::MODULE,"originalAndGeneratedPassed":true,"fromBytesHoldout":hold})
            );
        }
    }
    Ok(())
}
