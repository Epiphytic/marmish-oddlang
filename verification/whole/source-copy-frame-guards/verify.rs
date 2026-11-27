//! Recover the order of copy Data encode guards and compare four separately compiled libraries.
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
const DOMAIN: &str = "source-copy-frame-guards-v1";
const CASE: &str = "urn:marmish:copy:frame:Data";
const PACKAGE: &str = "stream-source-copy-frame-guards-package.json";
const METHOD: &str = "crate::copy_protocol::Frame::encode";
const BEFORE: &str = "                if bytes.is_empty() || bytes.len() > MAX_BLOCK {\n                    return Err(Error::Limit);\n                }\n";
const END: &str = "                crate::end(*n, bytes.len())?;\n";

fn derive(capture: &Path, cli: &Path, repo: &Path, host: &str, out: &Path) -> Result<Value> {
    fs::create_dir(out)?;
    let data = portable().join(DOMAIN);
    let binding = portable().join("copy-frame-v1/rust.json");
    let overlay = load(&binding)?;
    let mut recipe = load(&data.join("selection.json"))?;
    let mut variants = Vec::new();
    for case in recipe["cases"]
        .as_object()
        .ok_or("missing case selections")?
        .keys()
    {
        variants.push(text(&overlay["cases"][case])?);
    }
    variants.sort_unstable();
    let evidence = out.join("evidence.json");
    let selection = out.join("selection.json");
    let source = repo.join(SOURCE);
    let package = repo.join(CRATE);
    let captured = run(Command::new(capture)
        .arg(package.join("Cargo.toml"))
        .args(["marmish-stream", "lib", "marmish_stream", host, "default"])
        .env(
            "ODD_RECOVERY_TAGGED_FRAME_GUARDS",
            format!("{METHOD}#{}", variants.join(",")),
        ))?;
    fs::write(&evidence, captured)?;
    recipe["sourceSha256"] = json!(sha(&source)?);
    recipe["evidenceSha256"] = json!(sha(&evidence)?);
    fs::write(&selection, json_bytes(&recipe)?)?;
    let graph = out.join("graph.json");
    run(Command::new(cli)
        .args(["recover", "frame-guards"])
        .arg(package)
        .arg(source)
        .arg(evidence)
        .arg(selection)
        .arg(data.join("template.json"))
        .arg(binding)
        .arg(&graph))?;
    load(&graph)
}

fn oracle(mutated: bool) -> Vec<String> {
    let edge = i64::MAX as u64;
    let mut rows = Vec::new();
    for (offset, count) in [
        (edge + 1, 0usize),
        (edge + 1, 8193),
        (0, 0),
        (0, 8193),
        (edge, 1),
        (edge - 1, 1),
        (u64::MAX, 1),
        (edge - 2, 2),
        (edge, 8193),
        (0, 3),
        (0, 8192),
    ] {
        let bad_count = !(1..=8192).contains(&count);
        let bad_end = u128::from(offset) + count as u128 > u128::from(edge);
        let encode = if bad_end && (mutated || !bad_count) {
            "Offset".into()
        } else if bad_count {
            "Limit".into()
        } else {
            let mut wire = format!("05{:08x}{offset:016x}", count + 8);
            wire.push_str(&"76".repeat(count));
            wire
        };
        let decode = if offset > edge {
            "Offset"
        } else if bad_count {
            "Limit"
        } else if bad_end {
            "Offset"
        } else {
            "OK"
        };
        rows.push(format!("{encode} {decode}"));
    }
    rows.extend(std::iter::repeat_n("Offset".to_owned(), 7));
    rows
}

fn main() -> Result {
    let args = Args::parse();
    let experiment = FrameReplay::new(&args.original, &args.framework, "source-copy-frame-guards")?;
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
    let pair = format!("{BEFORE}{END}");
    require(
        src.matches(&pair).count() == 1,
        "ambiguous Data guard mutation",
    )?;
    fs::write(file, src.replacen(&pair, &format!("{END}{BEFORE}"), 1))?;
    let mut expected_graph = derive(&args.rust_capture, cli, original, host, base)?;
    let actual_graph = derive(&args.rust_capture, cli, mutant, host, changed)?;
    let data = portable().join(DOMAIN);
    let predecessor = portable().join("source-copy-frame-v1");
    for name in ["graph.json", "evidence.json", "selection.json"] {
        require(
            fs::read(base.join(name))? == fs::read(data.join(name))?,
            format!("baseline {name} differs"),
        )?;
    }
    require(
        fs::read(base.join("graph.json"))? == fs::read(predecessor.join("graph.json"))?,
        "prior frame graph changed",
    )?;
    let guards = expected_graph["cases"][CASE]["encodeGuards"]
        .as_array_mut()
        .ok_or("missing Data guards")?;
    require(guards.len() == 2, "expected two Data guards")?;
    guards.reverse();
    require(
        actual_graph == expected_graph,
        "mutation did not reverse only the Data encoder guards",
    )?;
    let mut remaining = load(&predecessor.join("obligations.json"))?;
    let recipe = load(&data.join("selection.json"))?;
    for case in recipe["cases"]
        .as_object()
        .ok_or("missing case selections")?
        .keys()
    {
        remaining["unresolved"]
            .as_object_mut()
            .ok_or("missing obligations")?
            .remove(case);
    }
    require(
        remaining == load(&data.join("remaining-obligations.json"))?,
        "remaining obligations differ",
    )?;
    let graph = changed.join("graph.json");
    let baseline = oracle(false);
    let mutation = oracle(true);
    let (generated, _) = experiment.qualify(
        PACKAGE,
        &data,
        &[("source-copy-frame-guards-v1/graph.json", &graph)],
        "observe-copy-guards",
        &baseline,
        &mutation,
    )?;
    let proof = load(&base.join("evidence.json"))?;
    let cases = proof["cases"].as_object().ok_or("missing evidence cases")?;
    let mut rules = 0;
    for c in cases.values() {
        rules += list(&c["guards"])?.len();
    }
    let report = json!({"original":ORIGINAL,"framework":fw,"rust":rust.trim_end(),"cases":cases.len(),"rules":rules,"checks":4*baseline.len(),
        "changedOutcomes":baseline.iter().zip(&mutation).filter(|(a,b)|a!=b).count(),"originalGraphSha256":sha(&base.join("graph.json"))?,
        "mutantGraphSha256":sha(&graph)?,"originalEvidenceSha256":sha(&base.join("evidence.json"))?,"mutantEvidenceSha256":sha(&changed.join("evidence.json"))?,
        "packageLockSha256":sha(&generated.join("package.lock"))?,"callerSha256":sha(&lane.join("caller.rs"))?,"sourceReadDenied":true,
        "remainingUnresolvedVariants":remaining["unresolved"].as_object().ok_or("missing remaining obligations")?.len()});
    fs::write(args.receipt, json_bytes(&report)?)?;
    Ok(())
}
