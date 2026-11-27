//! Regenerate the pinned Cargo acceptance inventory and its RDF obligations.
mod graph;
mod inventory;
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn list(v: &Value) -> Result<&Vec<Value>> {
    v.as_array().ok_or_else(|| "expected array".into())
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| "expected string".into())
}
fn strings(v: &Value) -> Result<Vec<&str>> {
    list(v)?.iter().map(string).collect()
}
fn object(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().ok_or_else(|| "expected object".into())
}
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program).args(args).current_dir(dir).output()?;
    if !out.status.success() {
        return Err(format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?)
}
fn encoded(mut data: Value) -> Result<String> {
    data.sort_all_objects();
    Ok(serde_json::to_string_pretty(&data)? + "\n")
}
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Generate {
        repo: PathBuf,
        spec: PathBuf,
        output: PathBuf,
    },
    Graph {
        spec: PathBuf,
        inventory: PathBuf,
        output: PathBuf,
    },
    Check {
        repo: PathBuf,
    },
}
fn check(repo: &Path) -> Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../whole-marmish/inventory");
    let mut spec = read(&fixture.join("release.json"))?;
    let data = inventory::generate(repo, &spec)?;
    if encoded(data.clone())?.as_bytes() != fs::read(fixture.join("inventory.json"))? {
        return Err("inventory drift".into());
    }
    if graph::project(&spec, &data)?.as_bytes() != fs::read(fixture.join("inventory.ttl"))? {
        return Err("inventory graph drift".into());
    }
    let stage = tempfile::tempdir()?;
    let id = format!(
        "{}:tools/v02-actions/release-schema.mjs",
        string(&spec["commit"])?
    );
    fs::write(
        stage.path().join("release-schema.mjs"),
        run(repo, "git", &["show", &id])?,
    )?;
    let version = inventory::package(&data["workspaces"], string(&spec["release"]["package"])?)?;
    let script = "import {releaseNames} from './release-schema.mjs'; console.log(JSON.stringify(releaseNames(process.argv[1])))";
    let names = run(
        stage.path(),
        "node",
        &[
            "--input-type=module",
            "-e",
            script,
            string(&version["version"])?,
        ],
    )?;
    if serde_json::from_str::<Value>(&names)? != data["release_assets"] {
        return Err("independent upstream split asset list differs".into());
    }
    spec["workspaces"] = json!([spec["workspaces"][0]]);
    match inventory::generate(repo, &spec) {
        Err(e) if e.to_string().contains("unclassified Cargo manifests") => (),
        other => {
            return Err(format!("standalone workspace omission was not detected: {other:?}").into());
        }
    }
    println!("Reproduced inventory, matched upstream asset list, detected omitted workspace.");
    Ok(())
}
fn main() -> Result<()> {
    match Args::parse().mode {
        Mode::Generate { repo, spec, output } => {
            fs::write(output, encoded(inventory::generate(&repo, &read(&spec)?)?)?)?
        }
        Mode::Graph {
            spec,
            inventory,
            output,
        } => fs::write(output, graph::project(&read(&spec)?, &read(&inventory)?)?)?,
        Mode::Check { repo } => check(&repo)?,
    }
    Ok(())
}
