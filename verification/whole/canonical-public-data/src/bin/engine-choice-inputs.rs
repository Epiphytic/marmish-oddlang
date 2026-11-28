//! Recover the two reviewed fieldless engine enums and stage their pinned package inputs.
#[path = "../../../engine-fieldless-choices/observer.rs"]
mod observer;
#[path = "../../../engine-fieldless-choices/qualification.rs"]
mod qualification;
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, pinned_copy},
    cli_projection::pinned,
    json_bytes, list, load, portable, require, run,
};
use regex::Regex;
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const MODEL: &str = "crates/marmish-engine/src/model.rs";
const MODEL_SHA: &str = "f147e2d08551d7d758a89b46857805820567a78f70571a663f33ea91672c9d53";
const DOMAIN: &str = "engine-fieldless-choices-v1";
const ID: &str = "urn:marmish:engine:fieldless-choices:v1";
const PACKAGE: &str = "engine-fieldless-choices-package.json";

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Observe(observer::Observer),
    Qualify(qualification::Args),
    Reference {
        checkout: PathBuf,
        host: String,
        target: PathBuf,
    },
    Checkout {
        directory: PathBuf,
        revision: String,
    },
    Compose {
        compiler: PathBuf,
        original: PathBuf,
        #[arg(long)]
        write: bool,
    },
    Prepare {
        compiler: PathBuf,
        model: PathBuf,
        inputs: PathBuf,
    },
    Mutate {
        original: PathBuf,
    },
}

fn span<'a>(model: &'a str, name: &str) -> Result<&'a str> {
    let pattern = Regex::new(&format!(
        r"(?m)^#\[derive\([^\r\n]+\)\]\npub enum {} \{{\n[^}}]*\}}\n",
        regex::escape(name)
    ))?;
    let mut matches = pattern.find_iter(model);
    let first = matches
        .next()
        .ok_or_else(|| format!("missing reviewed {name} span"))?;
    require(
        matches.next().is_none(),
        format!("ambiguous reviewed {name} span"),
    )?;
    Ok(first.as_str())
}
fn mutate(mut model: String) -> Result<String> {
    for (name, old, new) in [
        (
            "Stream",
            "    Stdout,\n    Stderr,",
            "    Stderr,\n    Stdout,",
        ),
        (
            "SubmitDisposition",
            "#[derive(Clone, Debug,",
            "#[derive(Clone, Copy, Debug,",
        ),
        (
            "SubmitDisposition",
            "    Accepted,\n    Existing,",
            "    Existing,\n    Accepted,",
        ),
    ] {
        let found = span(&model, name)?;
        require(
            found.matches(old).count() == 1,
            format!("ambiguous {name} edit"),
        )?;
        model = model.replacen(found, &found.replacen(old, new, 1), 1);
    }
    Ok(model)
}
fn sorted(mut v: Value) -> Result<Vec<u8>> {
    v.sort_all_objects();
    json_bytes(&v)
}
fn merge(into: &mut Value, from: &Value) -> Result {
    for (id, item) in from.as_object().ok_or("expected recovered object")? {
        require(
            into.get(id).is_none(),
            format!("duplicate recovered identity {id}"),
        )?;
        into[id] = item.clone();
    }
    Ok(())
}
struct Inputs {
    files: Vec<(PathBuf, Vec<u8>)>,
    inherited: Value,
    evidence: Value,
}
fn recover(compiler: &Path, model: &str) -> Result<Inputs> {
    let work = tempfile::tempdir()?;
    let mut types = json!({});
    let mut names = json!({});
    let mut variants = json!({});
    let mut evidence = json!({});
    for (name, suffix) in [
        ("Stream", "stream"),
        ("SubmitDisposition", "submit-disposition"),
    ] {
        let declaration = span(model, name)?;
        let src = work.path().join(format!("{suffix}.rs"));
        let selector = work.path().join(format!("{suffix}.json"));
        let output = work.path().join(suffix);
        fs::write(&src, declaration)?;
        fs::write(
            &selector,
            sorted(
                json!({"format":"oddlang-plain-choice-selection-v2","id":format!("{ID}:{suffix}"),"name":name,"sourceSha256":bytes_sha(declaration.as_bytes())}),
            )?,
        )?;
        run(Command::new(compiler)
            .args(["recover", "plain-choice"])
            .arg(src)
            .arg(selector)
            .arg(&output))?;
        merge(&mut types, &load(&output.join("graph.json"))?["types"])?;
        let binding = load(&output.join("rust.json"))?;
        merge(&mut names, &binding["types"])?;
        merge(&mut variants, &binding["variants"])?;
        evidence[name] = load(&output.join("observation.json"))?;
    }
    let graph = sorted(json!({"format":"oddlang-checked-values-v1","id":ID,"types":types}))?;
    let binding = sorted(
        json!({"format":"oddlang-checked-values-rust-v1","types":names,"variants":variants,"errors":{}}),
    )?;
    let base = load(&portable().join("engine-observational-records-package.json"))?;
    let mut spec = base.clone();
    let graph_path = Path::new(DOMAIN).join("graph.json");
    let binding_path = Path::new(DOMAIN).join("rust.json");
    spec["components"]
        .as_array_mut()
        .ok_or("missing package components")?
        .push(json!({
            "kind":"checked-values","module":"model","reexports":["Stream","SubmitDisposition"],
            "graph":{"path":graph_path,"sha256":bytes_sha(&graph)},
            "overlay":{"path":binding_path,"sha256":bytes_sha(&binding)}
        }));
    Ok(Inputs {
        files: vec![
            (graph_path, graph),
            (binding_path, binding),
            (PACKAGE.into(), json_bytes(&spec)?),
        ],
        inherited: base,
        evidence,
    })
}
fn write_file(path: &Path, data: &[u8]) -> Result {
    let parent = path.parent().ok_or("missing output parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    if path.exists() {
        file.as_file()
            .set_permissions(fs::metadata(path)?.permissions())?;
    }
    file.write_all(data)?;
    file.persist(path)?;
    Ok(())
}
fn original(path: &Path) -> Result<String> {
    Ok(String::from_utf8(pinned(path, MODEL, MODEL_SHA)?)?)
}
fn prepare(compiler: &Path, model: &Path, inputs: &Path) -> Result<Value> {
    let destination = std::path::absolute(inputs)?;
    let stage = tempfile::tempdir_in(destination.parent().ok_or("missing staging parent")?)?;
    let data = recover(compiler, &fs::read_to_string(model)?)?;
    let base = portable();
    pinned_copy(&base, &data.inherited["cargoLock"], stage.path())?;
    for component in list(&data.inherited["components"])? {
        for key in ["graph", "overlay"] {
            pinned_copy(&base, &component[key], stage.path())?;
        }
    }
    for (rel, bytes) in data.files {
        write_file(&stage.path().join(rel), &bytes)?;
    }
    renameat_with(CWD, stage.path(), CWD, destination, RenameFlags::NOREPLACE)?;
    Ok(data.evidence)
}
fn main() -> Result {
    match Args::parse().action {
        Action::Qualify(args) => qualification::qualify(args)?,
        Action::Checkout {
            directory,
            revision,
        } => public_data_qualification::assembly::checkout(&directory, &revision, true)?,
        Action::Reference {
            checkout,
            host,
            target,
        } => {
            let r = qualification::reference(Command::new("cargo"), &checkout, &host, &target)?;
            println!("{}", serde_json::to_string(&r.as_json())?);
        }
        Action::Observe(args) => println!("{}", serde_json::to_string(&args.observe()?)?),
        Action::Mutate { original: path } => print!("{}", mutate(original(&path)?)?),
        Action::Compose {
            compiler,
            original: path,
            write,
        } => {
            for (rel, data) in recover(&compiler, &original(&path)?)?.files {
                let dest = portable().join(rel);
                if write {
                    write_file(&dest, &data)?;
                } else {
                    require(
                        fs::read(&dest)? == data,
                        format!("sealed input differs: {}", dest.display()),
                    )?;
                }
            }
        }
        Action::Prepare {
            compiler,
            model,
            inputs,
        } => {
            println!(
                "{}",
                serde_json::to_string(&prepare(&compiler, &model, &inputs)?)?
            );
        }
    }
    Ok(())
}
