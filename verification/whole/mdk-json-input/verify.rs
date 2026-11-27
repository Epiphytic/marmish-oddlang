//! Recover supplied-byte JSON bounds from two native observations and qualify sealed Rust packages.
use clap::Parser;
use public_data_qualification::{
    assembly::{ORIGINAL, Sandbox, build_env, bytes_sha, checkout, pinned_copy, root},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const SOURCE: &str = "crates/marmish-mdk/src/lib.rs";
const TESTS: &str = "crates/marmish-mdk/src/tests.rs";
const CASE: &str = "tests::synthetic_json_and_ndjson_are_bounded_and_structural_only";
const PROBE: &str = "bounded_metadata_observation::discover_bounds";
const PREFIX: &str = "metadata::__projection_2::";
const DOMAIN: &str = "mdk-json-input-v1";
const MANIFEST: &str = "mdk-json-input-package.json";
const OBSERVER: &[u8] = include_bytes!("observe.rs");

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    compiler_target: PathBuf,
    original: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    prepare: bool,
}

fn write_json(path: &Path, mut value: Value) -> Result {
    value.sort_all_objects();
    fs::write(path, json_bytes(&value)?)?;
    Ok(())
}

fn test_output(work: &Path, target: &Path, name: &str) -> Result<String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(work)
        .args([
            "+1.97.1",
            "test",
            "--release",
            "--locked",
            "--offline",
            "--lib",
            "-p",
            "marmish-mdk",
            "--target-dir",
        ])
        .arg(target)
        .args(["--", "--exact", name, "--nocapture"]);
    let raw = stdout(build_env(&mut cmd))?;
    let passes: Vec<_> = raw
        .lines()
        .filter_map(|l| l.strip_prefix("test ")?.strip_suffix(" ... ok"))
        .collect();
    let summaries: Vec<_> = raw
        .lines()
        .filter(|l| l.starts_with("test result:"))
        .collect();
    require(
        passes == [name]
            && matches!(summaries.as_slice(),[s] if s.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;")),
        format!("expected exactly one execution of {name}"),
    )?;
    Ok(raw)
}

fn observe(work: &Path, target: &Path, name: &str) -> Result<Value> {
    let raw = test_output(work, target, name)?;
    let mut rows = raw
        .lines()
        .filter_map(|l| l.strip_prefix("BOUNDED_METADATA="));
    let observation =
        serde_json::from_str(rows.next().ok_or("missing bounded metadata observation")?)?;
    require(
        rows.next().is_none(),
        "duplicate bounded metadata observation",
    )?;
    Ok(observation)
}

struct Model {
    source_digest: String,
    observation: Value,
}

fn originals(args: &Args, scratch: &Path) -> Result<[Model; 2]> {
    let clone = scratch.join("pinned");
    run(Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--local",
            "--no-hardlinks",
            "--no-checkout",
        ])
        .arg(&args.original)
        .arg(&clone))?;
    run(Command::new("git")
        .current_dir(&clone)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    let source = clone.join(SOURCE);
    let old = fs::read_to_string(&source)?;
    let mut changed = old.clone();
    for (from, to) in [
        ("= Limits::NDJSON_BYTES;", "= Limits::NDJSON_BYTES / 2;"),
        ("records.len() >= 200", "records.len() >= 181"),
    ] {
        require(
            changed.matches(from).count() == 1,
            "ambiguous MDK mutation site",
        )?;
        changed = changed.replacen(from, to, 1);
    }
    let cache = std::path::absolute(&args.original_target)?
        .join("mdk-json-input")
        .join(ORIGINAL);
    test_output(&clone, &cache.join(bytes_sha(old.as_bytes())), CASE)?;
    let instrument = |body: &str| -> Result<Model> {
        let mut data = body.as_bytes().to_owned();
        data.push(b'\n');
        data.extend_from_slice(OBSERVER);
        fs::write(&source, &data)?;
        Ok(Model {
            source_digest: bytes_sha(body.as_bytes()),
            observation: observe(&clone, &cache.join(bytes_sha(&data)), PROBE)?,
        })
    };
    let baseline = instrument(&old)?;
    let mutant = instrument(&changed)?;
    let a = &baseline.observation;
    let b = &mutant.observation;
    let limit = b["acceptedValueBytes"]
        .as_u64()
        .ok_or("missing changed value limit")?;
    require(
        limit
            .checked_mul(2)
            .is_some_and(|size| a["acceptedValueBytes"] == size)
            && b["acceptedRecords"] == 181
            && a["acceptedRecords"] != 181
            && a["cases"] == b["cases"],
        "original source mutation did not discover both changed bounds",
    )?;
    Ok([baseline, mutant])
}

fn recover(cli: &Path, work: &Path, observed: &Value) -> Result<PathBuf> {
    fs::create_dir(work)?;
    let observation = work.join("observation.json");
    let selection = work.join("selection.json");
    let output = work.join("recovered");
    write_json(&observation, observed.clone())?;
    let error = "urn:marmish:mdk:adapter-error:v1:variant:";
    let capacity = format!("{error}OutputLimit");
    let invalid = format!("{error}InvalidResponse");
    write_json(
        &selection,
        json!({
            "format":"oddlang-bounded-json-selection-v1","id":"urn:marmish:mdk:synthetic-metadata:v1",
            "observationSha256":sha(&observation)?,"capacityError":capacity,"invalidError":invalid,
            "binding":{"format":"oddlang-bounded-json-rust-v1","single":"parse_json","sequence":"parse_ndjson","limit":"OUTPUT_LIMIT","errorType":"AdapterError","variants":{(capacity):"OutputLimit",(invalid):"InvalidResponse"}}
        }),
    )?;
    run(Command::new(cli)
        .args(["recover", "bounded-json-input"])
        .arg(observation)
        .arg(selection)
        .arg(&output))?;
    Ok(output)
}

fn compose(inputs: &Path, graph: &Path) -> Result<PathBuf> {
    let base = portable();
    let mut package = load(&base.join("mdk-public-data-package.json"))?;
    for part in list(&package["components"])? {
        for role in ["graph", "overlay"] {
            let source = pinned_copy(&base, &part[role], inputs)?;
            require(
                matches!(
                    source.extension().and_then(|ext| ext.to_str()),
                    Some("json" | "lock")
                ),
                "unexpected source input",
            )?;
        }
    }
    let dir = inputs.join(DOMAIN);
    fs::create_dir(&dir)?;
    for name in ["graph.json", "rust.json"] {
        fs::copy(graph.join(name), dir.join(name))?;
    }
    fs::copy(base.join(DOMAIN).join("Cargo.lock"), dir.join("Cargo.lock"))?;
    let binding = |name: &str| pin(inputs, &format!("{DOMAIN}/{name}"));
    package["cargoLock"] = binding("Cargo.lock")?;
    package["dependencies"] = json!({"serde_json":{"version":"1.0.151"}});
    package["components"].as_array_mut().ok_or("missing public components")?.push(json!({
        "kind":"bounded-json-input","module":"metadata","reexports":[],"graph":binding("graph.json")?,"overlay":binding("rust.json")?
    }));
    let manifest = inputs.join(MANIFEST);
    write_json(&manifest, package)?;
    Ok(manifest)
}

fn projected(
    sandbox: &Sandbox,
    work: &Path,
    cli: &Path,
    original: &Path,
    graph: &Path,
    baseline: bool,
) -> Result<Value> {
    fs::create_dir(work)?;
    let manifest = compose(work, graph)?;
    if baseline {
        require(
            fs::read(&manifest)? == fs::read(portable().join(MANIFEST))?,
            "baseline package differs from the published descriptor",
        )?;
    }
    let product = work.join("product");
    run(sandbox
        .command(work)?
        .arg(cli)
        .args(["build", "native-package", "rust"])
        .arg(manifest)
        .arg(&product))?;
    run(sandbox
        .command(&product)?
        .args(["cargo", "build", "--release", "--locked", "--offline"]))?;
    let digest = sha(&product.join("package.lock"))?;
    let leaf = product.join("src/group_2/__projection_2.rs");
    let mut code = fs::OpenOptions::new().append(true).open(leaf)?;
    code.write_all(b"\n")?;
    code.write_all(OBSERVER)?;
    drop(code);
    let target = product.join("target");
    if baseline {
        suite_stage::mdk(original, Some(&product))?;
        test_output(&product, &target, &format!("{PREFIX}{CASE}"))?;
    }
    Ok(
        json!({"packageLockSha256":digest,"observations":observe(&product,&target,&format!("{PREFIX}{PROBE}"))?,"recovery":load(&graph.join("recovery.lock"))?}),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, &args.framework_revision, false)?;
    let companion = stdout(
        Command::new("git")
            .current_dir(root())
            .args(["rev-parse", "HEAD"]),
    )?;
    if !args.prepare {
        checkout(root(), companion.trim(), false)?;
    }
    let target = std::path::absolute(&args.compiler_target)?;
    run(build_env(
        Command::new("cargo")
            .current_dir(&framework)
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(&target),
    ))?;
    let compiler = target.join("debug/oddlang").canonicalize()?;
    let scratch = tempfile::tempdir()?;
    let [baseline, mutation] = originals(&args, scratch.path())?;
    let base_graph = recover(
        &compiler,
        &scratch.path().join("baseline"),
        &baseline.observation,
    )?;
    let changed_graph = recover(
        &compiler,
        &scratch.path().join("changed"),
        &mutation.observation,
    )?;
    if args.prepare {
        let inputs = tempfile::tempdir()?;
        let manifest = compose(inputs.path(), &base_graph)?;
        for name in ["graph.json", "rust.json"] {
            fs::copy(base_graph.join(name), portable().join(DOMAIN).join(name))?;
        }
        fs::copy(manifest, portable().join(MANIFEST))?;
        return Ok(());
    }
    for name in ["graph.json", "rust.json"] {
        require(
            fs::read(base_graph.join(name))? == fs::read(portable().join(DOMAIN).join(name))?,
            "recovered graph differs from pin",
        )?;
    }
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rustc
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("unknown Rust host")?;
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let providers = work.join("providers");
    fs::create_dir(&providers)?;
    let sealed_cli = work.join("oddlang");
    fs::copy(&compiler, &sealed_cli)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[
            &root().canonicalize()?,
            &original,
            &target.canonicalize()?,
            &scratch.path().canonicalize()?,
        ],
        &providers,
    )?;
    let probes = [
        original.join(SOURCE),
        original.join(TESTS),
        whole().join("mdk-json-input/observe.rs"),
        scratch.path().join("pinned").join(SOURCE),
    ];
    sandbox.deny(&probes)?;
    let base = projected(
        &sandbox,
        &work.join("baseline"),
        &sealed_cli,
        &original,
        &base_graph,
        true,
    )?;
    let changed = projected(
        &sandbox,
        &work.join("changed"),
        &sealed_cli,
        &original,
        &changed_graph,
        false,
    )?;
    require(
        base["observations"] == baseline.observation
            && changed["observations"] == mutation.observation,
        "native recovery and projected controls disagree",
    )?;
    write_json(
        &args.receipt,
        json!({"format":"marmish-private-mdk-json-input-v1","original":ORIGINAL,"framework":args.framework_revision,
        "compilerSha256":sha(&compiler)?,"companion":companion.trim(),"rustc":rustc.lines().collect::<Vec<_>>(),"case":CASE,
        "caseSha256":suite_stage::mdk(&original,None)?["bodySha256"],"caseCount":1,"probeSha256":bytes_sha(OBSERVER),
        "originalSourceSha256":baseline.source_digest,"changedSourceSha256":mutation.source_digest,"baseline":base,"mutation":changed,"deniedSourcePaths":probes.len()}),
    )
}
