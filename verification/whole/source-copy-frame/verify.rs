//! Recover all top-level copy frame writes and compare the four native encoder outputs.
use clap::Parser;
use copy_checkpoint_replay::frame_pair::FrameReplay;
use public_data_qualification::{assembly::ORIGINAL, *};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    rust_capture: PathBuf,
    receipt: PathBuf,
}
const CRATE: &str = "crates/marmish-stream";
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";
const PACKAGE: &str = "stream-source-copy-frame-package.json";
const CASE: &str = "urn:marmish:copy:frame:Finish";
const BEFORE: &str = "            Self::Finish { transfer, sha256 } => {\n                payload.extend(transfer);\n                payload.extend(sha256);\n                9\n            }";
const AFTER: &str = "            Self::Finish { transfer, sha256 } => {\n                payload.extend(sha256);\n                payload.extend(transfer);\n                19\n            }";

fn recover(
    capture: &Path,
    cli: &Path,
    checkout: &Path,
    host: &str,
    dir: &Path,
    data: &Path,
) -> Result<Value> {
    fs::create_dir(dir)?;
    let evidence = dir.join("evidence.json");
    let package = checkout.join(CRATE);
    let source = checkout.join(SOURCE);
    let raw = run(Command::new(capture)
        .arg(package.join("Cargo.toml"))
        .args(["marmish-stream", "lib", "marmish_stream", host, "default"])
        .env(
            "ODD_RECOVERY_TAGGED_FRAME",
            "crate::copy_protocol::Frame::encode",
        ))?;
    fs::write(&evidence, raw)?;
    let mut recipe = load(&data.join("selection.json"))?;
    recipe["sourceSha256"] = json!(sha(&source)?);
    recipe["evidenceSha256"] = json!(sha(&evidence)?);
    let selection = dir.join("selection.json");
    fs::write(&selection, json_bytes(&recipe)?)?;
    let out = dir.join("recovered");
    run(Command::new(cli)
        .args(["recover", "tagged-frame"])
        .arg(package)
        .arg(source)
        .arg(evidence)
        .arg(selection)
        .arg(data.join("template.json"))
        .arg(portable().join("copy-frame-v1/rust.json"))
        .arg(&out))?;
    for name in ["graph.json", "obligations.json"] {
        fs::rename(out.join(name), dir.join(name))?;
    }
    load(&dir.join("graph.json"))
}

fn oracle(mutated: bool) -> Vec<String> {
    let transfer = "22".repeat(32);
    let digest = "45".repeat(32);
    let owners = format!("{transfer}{}{}", "38".repeat(32), "74".repeat(32));
    let metadata = "0100000002f18100000001de010000";
    let rows = [
        (1, format!("0000000101{owners}{metadata}01")),
        (
            2,
            format!("00000001{owners}{metadata}0000000000000654{digest}0000"),
        ),
        (3, format!("020000000000001234{digest}")),
        (4, format!("0000000000001334{digest}")),
        (5, "0000000000000016812659".into()),
        (6, format!("0000000000001534{digest}")),
        (7, format!("0000000000001634{digest}")),
        (8, format!("{transfer}0000000000001734{digest}01")),
        if mutated {
            (19, format!("{digest}{transfer}"))
        } else {
            (9, format!("{transfer}{digest}"))
        },
        (10, "020000000000000007000000000000000b".into()),
        (11, "0900".into()),
    ];
    rows.into_iter()
        .map(|(tag, payload)| format!("{tag:02x}{:08x}{payload}", payload.len() / 2))
        .collect()
}

fn projection(package: &Path) -> Result<String> {
    let lock = load(&package.join("package.lock"))?;
    let mut frames = list(&lock["components"])?
        .iter()
        .filter(|c| c["kind"] == "binary-frame");
    let frame = frames.next().ok_or("missing frame component")?;
    require(frames.next().is_none(), "ambiguous frame component")?;
    Ok(text(&frame["sourceSha256"])?.to_owned())
}
fn main() -> Result {
    let args = Args::parse();
    let experiment = FrameReplay::new(&args.original, &args.framework, "source-copy-frame")?;
    let FrameReplay {
        original,
        cli,
        mutant,
        base,
        changed,
        host,
        lane,
        rust,
        framework_pin: fw,
        ..
    } = &experiment;
    let file = mutant.join(SOURCE);
    let src = fs::read_to_string(&file)?;
    require(src.matches(BEFORE).count() == 1, "ambiguous Finish arm")?;
    fs::write(&file, src.replacen(BEFORE, AFTER, 1))?;
    let data = portable().join("source-copy-frame-v1");
    let baseline_graph = recover(&args.rust_capture, cli, original, host, base, &data)?;
    let changed_graph = recover(&args.rust_capture, cli, mutant, host, changed, &data)?;
    for name in [
        "graph.json",
        "evidence.json",
        "selection.json",
        "obligations.json",
    ] {
        require(
            fs::read(base.join(name))? == fs::read(data.join(name))?,
            format!("baseline {name} differs"),
        )?;
    }
    let mut expected = load(&portable().join("copy-frame-v1/graph.json"))?;
    for case in expected["cases"]
        .as_object_mut()
        .ok_or("missing frame cases")?
        .values_mut()
    {
        let obj = case.as_object_mut().ok_or("invalid frame case")?;
        for k in ["encodeGuards", "decodeGuards"] {
            obj.entry(k).or_insert(json!([]));
        }
    }
    require(baseline_graph == expected, "baseline frame layout differs")?;
    expected["cases"][CASE]["tag"] = json!(19);
    expected["cases"][CASE]["slots"]
        .as_array_mut()
        .ok_or("missing Finish fields")?
        .reverse();
    require(
        changed_graph == expected,
        "mutation changed unexpected frame facts",
    )?;
    let graph = changed.join("graph.json");
    let baseline = oracle(false);
    let mutation = oracle(true);
    let (generated, altered) = experiment.qualify(
        PACKAGE,
        &data,
        &[("source-copy-frame-v1/graph.json", &graph)],
        "observe-copy-frames",
        &baseline,
        &mutation,
    )?;
    let proof = load(&base.join("evidence.json"))?;
    let rows = list(&proof["cases"])?;
    let mut writes = 0;
    let mut unresolved = json!({});
    for row in rows {
        writes += list(&row["writes"])?.len();
        if !list(&row["unresolved"])?.is_empty() {
            unresolved[text(&row["variant"])?] = row["unresolved"].clone();
        }
    }
    let before = projection(&generated)?;
    let after = projection(&altered)?;
    require(before != after, "generated encoder did not change")?;
    let report = json!({
        "original": ORIGINAL, "framework": fw, "rust": rust.trim_end(), "cases": rows.len(),"writes":writes,
        "unresolved":unresolved,"checks":4*baseline.len(),"mutatedFrames":baseline.iter().zip(&mutation).filter(|(a,b)|a!=b).count(),
        "originalGraphSha256":sha(&base.join("graph.json"))?,"mutatedGraphSha256":sha(&graph)?,
        "originalEvidenceSha256":sha(&base.join("evidence.json"))?,"mutatedEvidenceSha256":sha(&changed.join("evidence.json"))?,
        "originalNativeArtifactSha256":before,"mutatedNativeArtifactSha256":after,"packageLockSha256":sha(&generated.join("package.lock"))?,
        "callerSha256":sha(&lane.join("caller.rs"))?,"sourceReadDenied":true
    });
    fs::write(args.receipt, json_bytes(&report)?)?;
    Ok(())
}
