//! Replay the source-derived record layout and the two-field writer mutation.
use clap::Parser;
use copy_checkpoint_replay::{build_compiler, compare, consumer, stage};
use copy_manifest_replay::launch_hash;
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, Sandbox, archive, checkout, root},
    json_bytes, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "f45f1308d995114740a8d55770f5c4ab3615bc40";
const MANIFEST: &str = "stream-launch-hash-package.json";
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

fn recover(
    src: &Path,
    cli: &Path,
    work: &Path,
    mut selection: Value,
    repin: bool,
) -> Result<Value> {
    fs::create_dir(work)?;
    let target = tempfile::tempdir()?;
    let expanded = work.join("expanded.rs");
    let output = Command::new("cargo")
        .current_dir(src)
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "+1.97.1",
            "rustc",
            "--locked",
            "--offline",
            "--lib",
            "-p",
            "marmish-stream",
            "--target-dir",
        ])
        .arg(target.path())
        .args(["--", "-Zunpretty=expanded"])
        .stdout(fs::File::create(&expanded)?)
        .status()?;
    require(output.success(), "native expansion failed")?;
    if repin {
        selection["sourceSha256"] = json!(sha(&expanded)?);
    }
    let pin = work.join("selection.json");
    fs::write(&pin, json_bytes(&selection)?)?;
    let observed = work.join("observed");
    run(Command::new(cli)
        .args(["recover", "record-writer"])
        .arg(&expanded)
        .arg(pin)
        .arg(&observed))?;
    fs::remove_file(expanded)?;
    require(
        load(&observed.join("observation.json"))?["resolutionGaps"] == json!([]),
        "unresolved record writes",
    )?;
    load(&observed.join("graph.json"))
}

fn replace_slots(contract: &mut Value, layout: &Value) -> Result {
    for (key, table) in layout["externalTags"]
        .as_object()
        .ok_or("missing tag tables")?
    {
        require(
            contract["hashTags"].get(key) == Some(table),
            "source byte tags disagree with release",
        )?;
    }
    let mut views = contract["hashViews"]
        .as_array_mut()
        .ok_or("missing views")?
        .iter_mut()
        .filter(|v| v["id"] == "urn:marmish:copy:launch:view:digest");
    let view = views.next().ok_or("missing digest view")?;
    require(
        views.next().is_none() && view["owner"] == layout["owner"],
        "ambiguous digest view or owner",
    )?;
    let mut fields: Vec<_> = layout["fields"]
        .as_array()
        .ok_or("missing fields")?
        .iter()
        .map(|slot| json!({"field":slot["field"],"encoding":slot["encoding"]}))
        .collect();
    fields.push(
        json!({"parameter":"urn:marmish:copy:launch:parameter:codec","encoding":"external-tag-u8"}),
    );
    view["fields"] = Value::Array(fields);
    Ok(())
}

fn mutate_writer(path: &Path) -> Result {
    let src = fs::read_to_string(path)?;
    let mut lines: Vec<_> = src.split_inclusive('\n').collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("fn encode_launch("))
        .ok_or("missing writer")?;
    let body = &mut lines[start..];
    let end = body
        .iter()
        .position(|l| l.trim() == "}")
        .ok_or("missing writer end")?;
    let field = |suffix: &str| -> Result<usize> {
        let mut hits = body[..end]
            .iter()
            .enumerate()
            .filter(|(_, l)| l.trim().ends_with(suffix));
        let (i, _) = hits.next().ok_or("missing writer slot")?;
        require(hits.next().is_none(), "duplicate writer slot")?;
        Ok(i)
    };
    let first = field(".extend(v.transfer);")?;
    let second = field(".extend(v.client);")?;
    body.swap(first, second);
    fs::write(path, lines.concat())?;
    Ok(())
}

fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, FRAMEWORK, false)?;
    let sources = tempfile::tempdir()?;
    let private = sources.path().canonicalize()?;
    let cli = build_compiler(&framework, &private.join("tools"), "+1.97.1")?;
    let data = portable().join("copy-launch-layout-v1");
    let selection = load(&data.join("selection.json"))?;
    let baseline = private.join("baseline");
    let layout = recover(&original, &cli, &baseline, selection.clone(), false)?;
    let observed = baseline.join("observed");
    for name in ["graph.json", "observation.json"] {
        require(
            fs::read(observed.join(name))? == fs::read(data.join("observed").join(name))?,
            format!("source observation differs: {name}"),
        )?;
    }
    let mutant = private.join("mutant");
    archive(&original, ORIGINAL, &mutant)?;
    mutate_writer(&mutant.join(SOURCE))?;
    let changed = recover(&mutant, &cli, &private.join("changed"), selection, true)?;
    require(
        changed["externalTags"] == layout["externalTags"],
        "mutation changed byte tags",
    )?;

    let public = tempfile::tempdir()?;
    let work = public.path().canonicalize()?;
    let (plan, mut manifest) = stage(&work.join("inputs"), MANIFEST, false)?;
    let pin = &mut manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?
        .last_mut()
        .ok_or("empty components")?["graph"];
    let contract_path = plan.parent().unwrap().join(text(&pin["path"])?);
    let mut contract = load(&contract_path)?;
    replace_slots(&mut contract, &layout)?;
    fs::write(&contract_path, json_bytes(&contract)?)?;
    let released = sha(&contract_path)?;
    require(
        pin["sha256"] == released,
        "recovered slots differ from release",
    )?;

    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing host")?;
    let cache = work.join("cache");
    fs::create_dir(&cache)?;
    let compiler = work.join("oddlang");
    fs::copy(&cli, &compiler)?;
    let companion = root().canonicalize()?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&original, &framework, &private, &companion],
        &cache,
    )?;
    let rows = launch_hash::rows();
    let requests: Vec<_> = rows.iter().map(|r| r["request"].clone()).collect();
    let expected: Vec<_> = rows.iter().map(|r| r["expected"].clone()).collect();
    let altered: Vec<_> = rows
        .iter()
        .map(|r| json!({"namespace":r["expected"]["namespace"],"digest":r["swapped"]["digest"]}))
        .collect();
    let caller = whole().join("copy-launch-hash/caller.rs");
    let verify = |label: &str, src: &Path, want: &[Value]| -> Result<String> {
        let dest = work.join(label);
        sandbox.deny(&[
            original.join("Cargo.toml"),
            framework.join("Cargo.toml"),
            mutant.join("Cargo.toml"),
            caller.clone(),
        ])?;
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
            "--release",
            "--offline",
        ]))?;
        for (role, package, lock) in [
            (
                "source",
                src.join("crates/marmish-stream"),
                src.join("Cargo.lock"),
            ),
            ("projection", dest.clone(), dest.join("Cargo.lock")),
        ] {
            let exe = consumer(
                &package,
                &private.join(format!("{label}-{role}")),
                &lock,
                &caller,
            )?;
            compare(&format!("{label}-{role}"), &exe, &requests, want)?;
        }
        sha(&dest.join("package.lock"))
    };
    let lock = verify("release", &original, &expected)?;
    replace_slots(&mut contract, &changed)?;
    fs::write(&contract_path, json_bytes(&contract)?)?;
    let changed_pin = sha(&contract_path)?;
    require(changed_pin != released, "slot reorder preserved contract")?;
    pin["sha256"] = json!(changed_pin);
    fs::write(&plan, json_bytes(&manifest)?)?;
    verify("reordered", &mutant, &altered)?;
    let different = expected
        .iter()
        .zip(&altered)
        .filter(|(a, b)| a != b)
        .count();
    require(different > 0, "slot reorder preserved native outputs")?;
    let receipt = json!({"reference":ORIGINAL,"framework":FRAMEWORK,"sourceSha256":sha(&original.join(SOURCE))?,
        "layoutSha256":sha(&observed.join("graph.json"))?,"observationSha256":sha(&observed.join("observation.json"))?,
        "contractSha256":released,"mutatedContractSha256":changed_pin,"mutatedCases":rows.len(),"changedResults":different,
        "packageLockSha256":lock,"recoveredFieldSlots":layout["fields"].as_array().unwrap().len(),"nativeCases":rows.len(),
        "platform":host,"originalAndCompanionReadDeniedDuringBuild":true});
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
