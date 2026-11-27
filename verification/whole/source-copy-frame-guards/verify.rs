//! Recover the order of copy Data encode guards and compare four separately compiled libraries.
use clap::Parser;
use copy_checkpoint_replay::{build_compiler, stage};
use public_data_qualification::{
    assembly::{ORIGINAL, Sandbox, archive, checkout, root},
    *,
};
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

fn project(
    sandbox: &Sandbox,
    work: &Path,
    cli: &Path,
    plan: &Path,
    out: &Path,
    denied: &[PathBuf],
) -> Result {
    sandbox.deny(denied)?;
    run(sandbox
        .command(work)?
        .arg(cli)
        .args(["build", "native-package", "rust"])
        .arg(plan)
        .arg(out))?;
    run(sandbox
        .command(out)?
        .args(["cargo", "build", "--locked", "--offline", "--release"]))?;
    Ok(())
}
fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let lane = whole().join("source-copy-frame-guards");
    let pin = load(&lane.join("reference-aarch64-apple-darwin.json"))?;
    let fw = text(&pin["framework"])?;
    checkout(&original, ORIGINAL, false)?;
    let framework_head = stdout(
        Command::new("git")
            .current_dir(&framework)
            .args(["rev-parse", "HEAD"]),
    )?;
    require(framework_head.trim() == fw, "framework revision differs")?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing host")?;
    let private = tempfile::tempdir()?;
    let scratch = private.path().canonicalize()?;
    let framework_source = scratch.join("framework");
    archive(&framework, fw, &framework_source)?;
    let cli = build_compiler(&framework_source, &scratch.join("tools"), "+1.97.1")?;
    let mutant = scratch.join("mutant");
    archive(&original, ORIGINAL, &mutant)?;
    let file = mutant.join(SOURCE);
    let src = fs::read_to_string(&file)?;
    let pair = format!("{BEFORE}{END}");
    require(
        src.matches(&pair).count() == 1,
        "ambiguous Data guard mutation",
    )?;
    fs::write(file, src.replacen(&pair, &format!("{END}{BEFORE}"), 1))?;
    let base = scratch.join("baseline-recovery");
    let changed = scratch.join("mutant-recovery");
    let mut expected_graph = derive(&args.rust_capture, &cli, &original, host, &base)?;
    let actual_graph = derive(&args.rust_capture, &cli, &mutant, host, &changed)?;
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
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let tools = work.join("oddlang");
    fs::copy(cli, &tools)?;
    let providers = work.join("provider-cache");
    fs::create_dir(&providers)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&root().canonicalize()?, &original, &framework, &scratch],
        &providers,
    )?;
    let denied = [
        original.join(SOURCE),
        mutant.join(SOURCE),
        lane.join("caller.rs"),
        data.join("evidence.json"),
        base.join("evidence.json"),
        changed.join("evidence.json"),
        framework.join("Cargo.toml"),
        framework_source.join("Cargo.toml"),
    ];
    let inputs = work.join("inputs");
    let (manifest, mut plan) = stage(&inputs, PACKAGE, false)?;
    require(
        !product::file_digests(&inputs)?.keys().any(|p| {
            p.ends_with(".rs")
                || Path::new(p)
                    .file_name()
                    .is_some_and(|n| n == "evidence.json")
        }),
        "source or evidence in projection inputs",
    )?;
    let generated = work.join("generated");
    let altered = work.join("altered");
    project(&sandbox, &work, &tools, &manifest, &generated, &denied)?;
    require(
        fs::read(generated.join("package.lock"))? == fs::read(data.join("package.lock"))?,
        "baseline package lock differs",
    )?;
    let frames = plan["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    let mut frames = frames.iter_mut().filter(|v| v["kind"] == "binary-frame");
    let frame = frames.next().ok_or("missing binary frame")?;
    require(frames.next().is_none(), "multiple binary frames")?;
    let graph = changed.join("graph.json");
    fs::copy(&graph, inputs.join(text(&frame["graph"]["path"])?))?;
    frame["graph"]["sha256"] = json!(sha(&graph)?);
    fs::write(&manifest, json_bytes(&plan)?)?;
    project(&sandbox, &work, &tools, &manifest, &altered, &denied)?;
    let baseline = oracle(false);
    let mutation = oracle(true);
    for (label, package, lock, expected) in [
        (
            "original",
            original.join(CRATE),
            original.join("Cargo.lock"),
            &baseline,
        ),
        (
            "original-mutated",
            mutant.join(CRATE),
            mutant.join("Cargo.lock"),
            &mutation,
        ),
        (
            "generated",
            generated.clone(),
            generated.join("Cargo.lock"),
            &baseline,
        ),
        (
            "generated-mutated",
            altered.clone(),
            altered.join("Cargo.lock"),
            &mutation,
        ),
    ] {
        let exe = stream_caller::build(
            &scratch.join(label),
            &package,
            &lock,
            include_bytes!("caller.rs"),
            "observe-copy-guards",
            "2021",
        )?;
        let raw = stdout(&mut Command::new(exe))?;
        require(
            raw.lines().eq(expected.iter().map(String::as_str)),
            format!("{label} guard observations differ"),
        )?;
    }
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
