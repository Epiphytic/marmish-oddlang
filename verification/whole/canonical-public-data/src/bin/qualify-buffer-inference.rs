//! Infer each reader contract from the original trace and test its isolated projection.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, bytes_sha},
    buffered::SOURCE,
    json_bytes, load, portable, private_buffer, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
#[path = "../../../private-buffer-inference/source.rs"]
mod traces;

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    compiler_cache: PathBuf,
    original_cargo_target: PathBuf,
    receipt: PathBuf,
}

fn selected<'a>(raw: &'a Value, id: &Value) -> Result<&'a Value> {
    let mut matches = raw["graph"]["declarations"]
        .as_array()
        .ok_or("missing compiler declarations")?
        .iter()
        .filter(|decl| decl.get("id") == Some(id));
    let record = matches.next().ok_or("missing selected declaration")?;
    require(matches.next().is_none(), "ambiguous selected declaration")?;
    Ok(record)
}

fn stage_pin(inputs: &Path, pin: &mut Value, replacement: Option<&[u8]>) -> Result<()> {
    let path = Path::new(text(&pin["path"])?);
    require(
        path.components()
            .all(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid portable input path",
    )?;
    let archived = fs::read(portable().join(path))?;
    require(
        pin["sha256"] == bytes_sha(&archived),
        "invalid portable input pin",
    )?;
    let data = replacement.unwrap_or(&archived);
    let dest = inputs.join(path);
    fs::create_dir_all(dest.parent().ok_or("missing input directory")?)?;
    fs::write(dest, data)?;
    pin["sha256"] = json!(bytes_sha(data));
    Ok(())
}

fn stage_package(inputs: &Path, graph: &[u8], baseline: bool) -> Result<()> {
    let archived = fs::read(portable().join(private_buffer::MANIFEST))?;
    let mut manifest: Value = serde_json::from_slice(&archived)?;
    stage_pin(inputs, &mut manifest["cargoLock"], None)?;
    let components = manifest["components"]
        .as_array_mut()
        .ok_or("missing package components")?;
    let [component] = components.as_mut_slice() else {
        return Err("expected a single reader component".into());
    };
    stage_pin(inputs, &mut component["graph"], Some(graph))?;
    stage_pin(inputs, &mut component["overlay"], None)?;
    let bytes = if baseline {
        archived
    } else {
        json_bytes(&manifest)?
    };
    fs::write(inputs.join(private_buffer::MANIFEST), bytes)?;
    Ok(())
}

struct Qualification {
    original: PathBuf,
    framework: PathBuf,
    cli: PathBuf,
    host: String,
    observations: Value,
    shape: PathBuf,
    selection: PathBuf,
    changed_source: PathBuf,
    archived_graph: Vec<u8>,
    archived_overlay: Vec<u8>,
}
impl Qualification {
    fn lane(&self, work: &Path, name: &str, changes: &[(&str, &str)]) -> Result<Value> {
        fs::create_dir(work)?;
        let observed = self.observations["lanes"]
            .get(name)
            .ok_or("missing original observation")?;
        let trace = observed.get("trace").ok_or("missing trace")?;
        let evidence = work.join("trace.json");
        fs::write(&evidence, json_bytes(trace)?)?;
        let out = work.join("inferred");
        run(Command::new(&self.cli)
            .args(["recover", "observed-buffered-input"])
            .arg(&self.shape)
            .arg(&self.selection)
            .arg(&evidence)
            .arg(&out))?;
        let graph_path = out.join("graph.json");
        let overlay_path = out.join("rust.json");
        let lock = out.join("recovery.lock");
        let proof = load(&lock)?;
        for (key, file) in [
            ("graphSha256", &graph_path),
            ("traceSha256", &evidence),
            ("selectionSha256", &self.selection),
            ("observationSha256", &self.shape),
            ("overlaySha256", &overlay_path),
        ] {
            require(
                proof[key] == sha(file)?,
                format!("{name}: recovery receipt differs for {key}"),
            )?;
        }
        let graph = fs::read(graph_path)?;
        let mut expected: Value = serde_json::from_slice(&self.archived_graph)?;
        for (key, value) in changes {
            require(
                expected["transitions"].get(*key).is_some(),
                "unknown policy transition",
            )?;
            expected["transitions"][key] = json!(value);
        }
        require(
            serde_json::from_slice::<Value>(&graph)? == expected
                && fs::read(overlay_path)? == self.archived_overlay
                && (!changes.is_empty() || graph == self.archived_graph),
            format!("{name}: inferred contract differs"),
        )?;
        let sealed = if cfg!(target_os = "linux") {
            tempfile::tempdir_in("/tmp")?
        } else {
            tempfile::tempdir()?
        };
        let root = sealed.path().canonicalize()?;
        let inputs = root.join("inputs");
        fs::create_dir(&inputs)?;
        stage_package(&inputs, &graph, changes.is_empty())?;
        let observer = whole().join("private-buffer-inference/trace.rs");
        let denied = [
            evidence.clone(),
            self.shape.clone(),
            self.changed_source.clone(),
            observer.clone(),
        ];
        let (product, package_lock) = private_buffer::project(
            &root,
            &inputs,
            &self.cli,
            &self.original,
            &self.framework,
            &self.host,
            &denied,
        )?;
        let mut leaf = fs::OpenOptions::new()
            .append(true)
            .open(product.join("src/group_0/__projection_0.rs"))?;
        leaf.write_all(b"\n")?;
        leaf.write_all(&fs::read(observer)?)?;
        let emitted = traces::capture(
            &product,
            &work.join("test-target"),
            &self.host,
            "buffer::__projection_0::portable_buffer_behavior::collect",
        )?;
        require(
            &emitted == trace,
            format!("{name}: projected reader differs from original trace"),
        )?;
        Ok(
            json!({"graphSha256":bytes_sha(&graph),"packageLockSha256":package_lock,"recoveryLockSha256":sha(&lock)?,
            "traceSha256":sha(&evidence)?,"sourceSha256":observed["sourceSha256"],"selectionSha256":sha(&self.selection)?,"generatedTraceMatched":true}),
        )
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let dir = whole().join("private-buffer-inference");
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let pin = load(&dir.join("compiler.json"))?;
    let pin = text(&pin["revision"])?;
    assembly::checkout(&framework, pin, false)?;
    assembly::checkout(&original, ORIGINAL, false)?;
    let cache = std::path::absolute(args.compiler_cache)?;
    run(assembly::build_env(
        Command::new("cargo")
            .current_dir(&framework)
            .args([
                "+1.96.1",
                "build",
                "--locked",
                "--offline",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(&cache),
    ))?;
    let original_target = std::path::absolute(args.original_cargo_target)?;
    let host = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = host
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing native host")?
        .to_owned();
    let observations = traces::observe(&original, &original_target)?;
    let source_archive = dir.join(format!("original-{host}.json"));
    require(
        observations == load(&source_archive)?,
        "source behavior differs from pinned observations",
    )?;
    let scratch = tempfile::tempdir()?;
    let work = scratch.path().canonicalize()?;
    let changed = work.join("reference-mutant");
    require(
        traces::prepare(&original, &changed)? == observations["lanes"]["combined"]["sourceSha256"],
        "combined mutation source differs",
    )?;
    let raw = private_buffer::capture(&framework, &original, &host)?;
    let altered = private_buffer::capture(&framework, &changed, &host)?;
    let selection = dir.join("shape-selection.json");
    let id = load(&selection)?
        .get("declaration")
        .ok_or("missing declaration selector")?
        .clone();
    require(
        selected(&serde_json::from_slice(&raw)?, &id)?
            == selected(&serde_json::from_slice(&altered)?, &id)?,
        "combined mutations changed selected storage declaration",
    )?;
    let shape = work.join("shapes.json");
    fs::write(&shape, raw)?;
    let q = Qualification {
        original,
        framework,
        cli: cache.join("debug/oddlang"),
        host,
        observations,
        shape,
        selection,
        changed_source: changed.join(SOURCE),
        archived_graph: fs::read(portable().join("prototype-buffer-v1/graph.json"))?,
        archived_overlay: fs::read(portable().join("prototype-buffer-v1/rust.json"))?,
    };
    let empty = ("emptyRead", "return-zero");
    let consume = ("consumedBytes", "retain");
    let error = ("refillError", "discard-window");
    let mut lanes = serde_json::Map::new();
    for (name, changes) in [
        ("original", &[][..]),
        ("empty", &[empty]),
        ("consume", &[consume]),
        ("error", &[error]),
        ("combined", &[empty, consume, error]),
    ] {
        lanes.insert(name.into(), q.lane(&work.join(name), name, changes)?);
    }
    let report = json!({"original":ORIGINAL,"target":q.host,"originalObservationsSha256":sha(&source_archive)?,"shapeUnchangedUnderCombinedMutation":true,"lanes":lanes,"framework":pin});
    fs::write(args.receipt, json_bytes(&report)?)?;
    Ok(())
}
