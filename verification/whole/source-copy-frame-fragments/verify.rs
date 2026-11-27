//! Recover the nested Start and Manifest writers and verify the native encodings.
use clap::Parser;
use copy_checkpoint_replay::frame_pair::FrameReplay;
use public_data_qualification::{assembly::ORIGINAL, *};
use serde_json::json;
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
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";
const COMPONENTS: [(&str, &str, &str); 2] = [
    ("launch", "Start", "copy-frame-v1/launch-fragment-rust.json"),
    ("manifest", "Manifest", "copy-manifest-v1/rust.json"),
];
fn mutate(file: &Path) -> Result {
    let mut source = fs::read_to_string(file)?;
    let before = "    out.extend(v.client);\n    out.extend(v.server);";
    let after = "    out.extend(v.server);\n    out.extend(v.client);";
    require(
        source.matches(before).count() == 2,
        "unexpected number of owner pairs",
    )?;
    for (name, _, _) in COMPONENTS {
        let start = source
            .find(&format!("fn encode_{name}("))
            .ok_or("missing fragment encoder")?;
        let stop = start + source[start..].find("\n}").ok_or("missing encoder body")?;
        let body = &source[start..stop];
        require(
            body.matches(before).count() == 1,
            "missing unique owner pair in encoder",
        )?;
        source.replace_range(start..stop, &body.replacen(before, after, 1));
    }
    fs::write(file, source)?;
    Ok(())
}

fn derive(
    experiment: &FrameReplay,
    capture: &Path,
    checkout: &Path,
    dir: &Path,
    data: &Path,
) -> Result {
    fs::create_dir(dir)?;
    let package = checkout.join("crates/marmish-stream");
    let source = checkout.join(SOURCE);
    let evidence = dir.join("evidence.json");
    fs::write(
        &evidence,
        run(Command::new(capture)
            .arg(package.join("Cargo.toml"))
            .args([
                "marmish-stream",
                "lib",
                "marmish_stream",
                &experiment.host,
                "default",
            ])
            .env(
                "ODD_RECOVERY_TAGGED_FRAME_FRAGMENTS",
                "crate::copy_protocol::Frame::encode#Start,Manifest",
            ))?,
    )?;
    for (name, variant, binding) in COMPONENTS {
        let filename = format!("{name}-selection.json");
        let selection = dir.join(&filename);
        let mut recipe = load(&data.join(filename))?;
        require(recipe["variant"] == variant, "wrong selected variant")?;
        recipe["sourceSha256"] = json!(sha(&source)?);
        recipe["evidenceSha256"] = json!(sha(&evidence)?);
        fs::write(&selection, json_bytes(&recipe)?)?;
        let out = dir.join(format!("{name}-recovery"));
        run(Command::new(&experiment.cli)
            .args(["recover", "frame-fragment"])
            .arg(&package)
            .arg(&source)
            .arg(&evidence)
            .arg(selection)
            .arg(data.join(format!("{name}-template.json")))
            .arg(portable().join(binding))
            .arg(&out))?;
        fs::rename(out.join("graph.json"), dir.join(format!("{name}.json")))?;
        fs::rename(
            out.join("dependencies.json"),
            dir.join(format!("{name}-dependencies.json")),
        )?;
    }
    Ok(())
}

fn check_release(exp: &FrameReplay, data: &Path) -> Result {
    require(
        fs::read(exp.base.join("evidence.json"))? == fs::read(data.join("evidence.json"))?,
        "baseline evidence differs",
    )?;
    for (name, _, _) in COMPONENTS {
        for suffix in [".json", "-selection.json", "-dependencies.json"] {
            let file = format!("{name}{suffix}");
            require(
                fs::read(exp.base.join(&file))? == fs::read(data.join(&file))?,
                format!("baseline {file} differs"),
            )?;
        }
        let graph = format!("{name}.json");
        let mut released = load(&portable().join(format!("copy-frame-v1/{name}-fragment.json")))?;
        require(
            load(&exp.base.join(&graph))? == released,
            "baseline fragment layout differs",
        )?;
        let slots = released["layout"].as_array_mut().ok_or("missing layout")?;
        let position = |suffix: &str| -> Result<usize> {
            let mut indices = slots
                .iter()
                .enumerate()
                .filter(|(_, v)| v["id"].as_str().is_some_and(|s| s.ends_with(suffix)))
                .map(|(i, _)| i);
            let index = indices.next().ok_or("missing owner field")?;
            require(indices.next().is_none(), "ambiguous owner field")?;
            Ok(index)
        };
        let (client, server) = (position(":client")?, position(":server")?);
        slots.swap(client, server);
        require(
            load(&exp.changed.join(graph))? == released,
            "mutation changed unexpected layout facts",
        )?;
        let dependencies = format!("{name}-dependencies.json");
        require(
            fs::read(exp.changed.join(&dependencies))? == fs::read(data.join(dependencies))?,
            "mutation changed fragment dependencies",
        )?;
    }
    Ok(())
}
fn oracle(mutated: bool) -> Vec<String> {
    let transfer = "12".repeat(32);
    let client = "34".repeat(32);
    let server = "56".repeat(32);
    let owners = if mutated {
        format!("{transfer}{server}{client}")
    } else {
        format!("{transfer}{client}{server}")
    };
    let fields = format!("{owners}0100000002abcd00000001e1");
    let start = format!("0000000101{fields}010000");
    let manifest = format!("00000001{fields}");
    let end = format!("0000000000000213{}", "78".repeat(32));
    let frame = |tag: u8, payload: String| format!("{tag:02x}{:08x}{payload}", payload.len() / 2);
    let invalid = |field: &str| format!("Invalid(\"copy_{field}\")");
    vec![
        frame(1, format!("{start}00")),
        frame(1, format!("{start}01")),
        invalid("version"),
        invalid("path"),
        frame(2, format!("{manifest}010000{end}0000")),
        frame(
            2,
            format!("{manifest}010101{end}01000001a001fffffffffffffffd00000009"),
        ),
        invalid("version"),
        invalid("path"),
        invalid("metadata"),
    ]
}
fn main() -> Result {
    let args = Args::parse();
    let exp = FrameReplay::new(
        &args.original,
        &args.framework,
        "source-copy-frame-fragments",
    )?;
    let data = portable().join("source-copy-frame-fragments-v1");
    mutate(&exp.mutant.join(SOURCE))?;
    derive(&exp, &args.rust_capture, &exp.original, &exp.base, &data)?;
    derive(&exp, &args.rust_capture, &exp.mutant, &exp.changed, &data)?;
    check_release(&exp, &data)?;
    let baseline = oracle(false);
    let mutation = oracle(true);
    let (original, altered) = exp.qualify(
        "stream-source-copy-frame-fragments-package.json",
        &data,
        &[
            (
                "source-copy-frame-fragments-v1/launch.json",
                &exp.changed.join("launch.json"),
            ),
            (
                "source-copy-frame-fragments-v1/manifest.json",
                &exp.changed.join("manifest.json"),
            ),
        ],
        "observe-copy-fragments",
        &baseline,
        &mutation,
    )?;
    let evidence = load(&exp.base.join("evidence.json"))?;
    let mut slots = 0;
    for case in evidence["cases"]
        .as_object()
        .ok_or("missing cases")?
        .values()
    {
        slots += list(&case["slots"])?.len();
    }
    let graph_hashes = |dir: &Path| -> Result<serde_json::Value> {
        let mut hashes = json!({});
        for (name, _, _) in COMPONENTS {
            hashes[name] = json!(sha(&dir.join(format!("{name}.json")))?);
        }
        Ok(hashes)
    };
    let report = json!({
        "original":ORIGINAL,"framework":exp.framework_pin,"rust":exp.rust,"variants":COMPONENTS.len(),"slots":slots,
        "observations":4*baseline.len(),"changedOutcomes":baseline.iter().zip(&mutation).filter(|(a,b)|a!=b).count(),
        "originalEvidenceSha256":sha(&exp.base.join("evidence.json"))?,"mutatedEvidenceSha256":sha(&exp.changed.join("evidence.json"))?,
        "originalGraphs":graph_hashes(&exp.base)?,"mutatedGraphs":graph_hashes(&exp.changed)?,
        "originalPackageLockSha256":sha(&original.join("package.lock"))?,"mutatedPackageLockSha256":sha(&altered.join("package.lock"))?,
        "callerSha256":sha(&exp.lane.join("caller.rs"))?,"sourceReadDenied":true
    });
    fs::write(args.receipt, json_bytes(&report)?)?;
    Ok(())
}
