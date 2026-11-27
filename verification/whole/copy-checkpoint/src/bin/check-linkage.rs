use clap::Parser;
use copy_checkpoint_replay::{build_compiler, python_line, stage};
use public_data_qualification::{
    Result, assembly::bytes_sha, capture, json_bytes, list, load, portable, require, run, sha, text,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FRAMEWORK: &str = "70821c898cabe15a5d0483dc9425e51aae883932";
const SOURCE: &str = "stream-package-v5.json";
const GRAPH: &str = "copy-checkpoint-v1/graph.json";
const OVERLAY: &str = "copy-checkpoint-v1/rust.json";
const NESTED: &str = "urn:marmish:copy:manifest";
#[derive(Parser)]
struct Args {
    framework: PathBuf,
    receipt: PathBuf,
}
fn verify(cli: &Path, dir: &Path) -> Result<Value> {
    let (path, manifest) = stage(dir, SOURCE, true)?;
    let release = load(&portable().join("workspace-v5/workspace.lock"))?;
    let sealed = &release["members"]["marmish-stream"];
    require(
        sha(&path)? == sealed["inputSha256"],
        "v5 package input pin differs",
    )?;
    let component = list(&manifest["components"])?
        .iter()
        .position(|c| c["graph"]["path"] == GRAPH)
        .ok_or("missing checkpoint component")?;
    let graph = load(&dir.join(GRAPH))?;
    let overlay = load(&dir.join(OVERLAY))?;
    let mut results = json!({});
    let mut build =
        |name: &str, g: Option<&Value>, b: Option<&Value>, error: Option<&str>| -> Result {
            let mut m = manifest.clone();
            let c = &mut m["components"][component];
            for (key, changed) in [("graph", g), ("overlay", b)] {
                let rel = text(&c[key]["path"])?;
                let data = match changed {
                    Some(v) => python_line(v)?,
                    None => fs::read(portable().join(rel))?,
                };
                fs::write(dir.join(rel), &data)?;
                c[key]["sha256"] = json!(bytes_sha(&data));
            }
            let data = if name == "released" {
                fs::read(portable().join(SOURCE))?
            } else {
                python_line(&m)?
            };
            fs::write(&path, data)?;
            let out = dir.join(name);
            let result = capture(
                Command::new(cli)
                    .current_dir(dir)
                    .env("RUSTUP_TOOLCHAIN", "1.97.1")
                    .args(["build", "native-package", "rust"])
                    .arg(&path)
                    .arg(&out),
                600,
                None,
            )?;
            let diagnostic = std::str::from_utf8(&result.stderr)?.trim();
            if let Some(expected) = error {
                require(
                    result.status.code() == Some(1)
                        && diagnostic.contains(expected)
                        && !out.exists(),
                    format!("{name}: {diagnostic}"),
                )?;
                results[name] = json!(diagnostic);
            } else {
                require(result.status.success(), diagnostic)?;
                results[name] = json!(sha(&out.join("package.lock"))?);
            }
            Ok(())
        };
    build("released", None, None, None)?;
    let mut explicit = graph.clone();
    explicit["externalRecords"][NESTED]["codecMaximum"] = json!(16384);
    build("pinned-inner-limit", Some(&explicit), None, None)?;
    explicit["externalRecords"][NESTED]["codecMaximum"] = json!(16383);
    build(
        "wrong-inner-limit",
        Some(&explicit),
        None,
        Some("nested codec maximum"),
    )?;
    for field in ["size", "sha256"] {
        let mut changed = overlay.clone();
        changed["externalRecords"][NESTED]["fields"][format!("{NESTED}:field:{field}")] =
            json!("other");
        build(
            &format!("wrong-{field}-binding"),
            None,
            Some(&changed),
            Some("nested codec member"),
        )?;
    }
    let mut capped = graph.clone();
    let slot = capped["layout"]
        .as_array_mut()
        .ok_or("missing layout")?
        .iter_mut()
        .find(|s| s["encoding"]["record"] == NESTED)
        .ok_or("missing nested slot")?;
    slot["encoding"]["maximum"] = json!(1);
    build(
        "impossible-slot",
        Some(&capped),
        None,
        Some("nested codec minimum"),
    )?;
    let mut other = overlay;
    other["externalRecords"][NESTED]["path"] = json!("crate::copy_protocol::HelperContext");
    build(
        "wrong-record",
        None,
        Some(&other),
        Some("nested codec identity"),
    )?;
    require(
        results["released"] == sealed["standalonePackageLockSha256"],
        "released package lock differs",
    )?;
    Ok(json!({"releaseSha256":sealed["inputSha256"],"results":results,"framework":FRAMEWORK}))
}
fn main() -> Result {
    let a = Args::parse();
    let t = tempfile::tempdir()?;
    let work = t.path().canonicalize()?;
    let archive = work.join("framework.tar");
    let source = work.join("compiler");
    fs::create_dir(&source)?;
    run(Command::new("git")
        .current_dir(a.framework)
        .args(["archive", "--output"])
        .arg(&archive)
        .arg(FRAMEWORK))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(&source))?;
    let cli = build_compiler(&source, &work.join("target"), "+1.96.1")?;
    let mut report = verify(&cli, &work.join("inputs"))?;
    report.sort_all_objects();
    fs::write(a.receipt, json_bytes(&report)?)?;
    Ok(())
}
