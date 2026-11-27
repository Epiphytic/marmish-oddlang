//! Observe the upstream release manifest parser on a pinned temporary source tree.
use clap::Parser;
use public_data_qualification::{Result, capture, json_bytes, load, require, sha, text, whole};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const REVISION: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
const INPUTS_SHA: &str = "967cbae1e747bbb6b2aae7007f4240d8b273a2fa8a73f6a5b016efa6685d3cdf";
const CHECKS: [&str; 2] = [
    "native_manifest::tests::strict_vectors",
    "native_manifest::tests::shared_release_version_vectors",
];
const PROBE: &str = "manifest_oracle::manifest_contract_probe";
#[derive(Parser)]
struct Args {
    original: PathBuf,
    output: PathBuf,
    #[arg(long)]
    target_dir: PathBuf,
}
fn output(c: &mut Command) -> Result<Vec<u8>> {
    let o = capture(c, 1200, None)?;
    require(
        o.status.success(),
        format!(
            "{c:?} exited {}: {} {}",
            o.status,
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )?;
    Ok(o.stdout)
}
fn conformance_tool(lane: &Path) -> Result<PathBuf> {
    let stdout = output(
        Command::new("cargo")
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "--manifest-path",
            ])
            .arg(lane.join("Cargo.toml"))
            .args([
                "--bin",
                "release-manifest-conformance",
                "--message-format=json-render-diagnostics",
            ]),
    )?;
    let mut exes = Vec::new();
    for line in std::str::from_utf8(&stdout)?.lines() {
        let r: Value = serde_json::from_str(line)?;
        if r["reason"] == "compiler-artifact"
            && r["target"]["name"] == "release-manifest-conformance"
            && let Some(exe) = r["executable"].as_str()
        {
            exes.push(PathBuf::from(exe))
        }
    }
    require(
        exes.len() == 1,
        "expected one native release-manifest-conformance executable",
    )?;
    Ok(exes.remove(0))
}
fn corpus(tool: &Path, mode: &str, path: &Path) -> Result {
    output(Command::new(tool).arg(mode).arg(path))?;
    Ok(())
}
fn prepare_inputs(tool: &Path, work: &Path) -> Result<PathBuf> {
    let generated = work.join("corpus-inputs.json");
    corpus(tool, "inputs", &generated)?;
    let doc = load(&generated)?;
    let mut buffer = String::from("[");
    for (i, v) in doc
        .as_array()
        .ok_or("expected input rows")?
        .iter()
        .enumerate()
    {
        // The original native test received these ordered objects as one ASCII line.
        if i > 0 {
            buffer.push_str(", ");
        }
        buffer.push_str(&format!(
            "{{\"case\": {}, \"hex\": {}}}",
            serde_json::to_string(text(&v["case"])?)?,
            serde_json::to_string(text(&v["hex"])?)?
        ));
    }
    buffer.push(']');
    let path = work.join("cases.json");
    fs::write(&path, buffer)?;
    require(
        sha(&path)? == INPUTS_SHA,
        "historical native input bytes changed",
    )?;
    Ok(path)
}
fn select(
    tree: &Path,
    target: &Path,
    name: &str,
    observation: Option<(&Path, &Path)>,
) -> Result<String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(tree)
        .args([
            "+1.97.1",
            "test",
            "--release",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(target)
        .args(["-p", "marmish-native", "--lib", name, "--", "--exact"])
        .env("CARGO_NET_OFFLINE", "true")
        .env("MARMISH_BUILD_COMMIT", ORIGINAL);
    for key in [
        "CARGO_BUILD_TARGET",
        "CARGO_TARGET_DIR",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        cmd.env_remove(key);
    }
    if let Some((cases, result)) = observation {
        cmd.env("MANIFEST_CASES", cases)
            .env("MANIFEST_RESULT", result);
    }
    let stdout = String::from_utf8(output(&mut cmd)?)?;
    let witness = format!("test {name} ... ok");
    require(
        stdout.lines().filter(|l| *l == witness).count() == 1
            && stdout.contains("1 passed; 0 failed"),
        format!("missing exact native test {name}"),
    )?;
    Ok(witness)
}
fn observe(
    tool: &Path,
    tree: &Path,
    target: &Path,
    inputs: &Path,
    mode: &str,
    result: &Path,
) -> Result<Vec<Value>> {
    select(tree, target, PROBE, Some((inputs, result)))?;
    corpus(tool, mode, result)?;
    Ok(serde_json::from_slice(&fs::read(result)?)?)
}
fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let target = std::path::absolute(&args.target_dir)?;
    let lane = whole().join("release-manifest");
    require(
        std::str::from_utf8(&output(
            Command::new("git")
                .arg("-C")
                .arg(&original)
                .args(["rev-parse", "HEAD"]),
        )?)?
        .trim()
            == ORIGINAL,
        "unexpected pinned upstream revision",
    )?;
    let rustc = String::from_utf8(output(Command::new("rustc").args(["+1.97.1", "-vV"]))?)?;
    require(
        rustc
            .lines()
            .any(|line| line == format!("commit-hash: {REVISION}")),
        "unexpected upstream compiler",
    )?;
    let tool = conformance_tool(&lane)?;
    let tmp = tempfile::Builder::new()
        .prefix("odd-manifest-oracle-")
        .tempdir()?;
    let work = tmp.path();
    let tree = work.join("original");
    fs::create_dir(&tree)?;
    let archive = work.join("source.tar");
    output(
        Command::new("git")
            .arg("-C")
            .arg(&original)
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(ORIGINAL),
    )?;
    output(
        Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&tree),
    )?;
    let mut source_pins = serde_json::Map::new();
    for name in [
        SOURCE,
        "crates/marmish-native/src/foundation.rs",
        "crates/marmish-native/src/error.rs",
        "tools/release-version-vectors.json",
    ] {
        let data = output(
            Command::new("git")
                .arg("-C")
                .arg(&original)
                .args(["show", &format!("{ORIGINAL}:{name}")]),
        )?;
        let digest = format!("{:x}", Sha256::digest(data));
        require(
            sha(&tree.join(name))? == digest,
            "archived source differs from pinned Git blob",
        )?;
        source_pins.insert(name.into(), json!(digest));
    }
    let native = tree.join("crates/marmish-native/src");
    OpenOptions::new()
        .append(true)
        .open(native.join("lib.rs"))?
        .write_all(b"\n#[cfg(test)]\n#[path = \"manifest_oracle.rs\"]\nmod manifest_oracle;\n")?;
    fs::copy(lane.join("probe.rs"), native.join("manifest_oracle.rs"))?;
    let inputs = prepare_inputs(&tool, work)?;
    let mut witnesses = vec![];
    for name in CHECKS {
        witnesses.push(select(&tree, &target, name, None)?);
    }
    let baseline = observe(
        &tool,
        &tree,
        &target,
        &inputs,
        "check",
        &work.join("baseline.json"),
    )?;
    let source = tree.join(SOURCE);
    let original_source = fs::read_to_string(&source)?;
    let before = "                    16 * 1024 * 1024\n";
    let after = "                    16 * 1024 * 1024 + 1\n";
    require(
        original_source.matches(before).count() == 1,
        "mutation requires exactly one license ceiling",
    )?;
    fs::write(&source, original_source.replacen(before, after, 1))?;
    let changed = observe(
        &tool,
        &tree,
        &target,
        &inputs,
        "check-changed",
        &work.join("mutated.json"),
    )?;
    require(
        baseline.len() == changed.len(),
        "mutation row count changed",
    )?;
    let delta: Vec<_> = baseline
        .iter()
        .zip(&changed)
        .filter(|(a, b)| a != b)
        .map(|(_, b)| b)
        .collect();
    require(
        delta.len() == 1 && delta[0]["case"] == "size-3-16777217",
        "unexpected changed cases",
    )?;
    let report = json!({"format":"marmish-release-manifest-original-observation-v1","originalRevision":ORIGINAL,"compiler":rustc.lines().collect::<Vec<_>>(),"sourcePins":source_pins,
        "mutatedSourceSha256":sha(&source)?,"callerSha256":sha(&lane.join("probe.rs"))?,"corpusSha256":sha(&lane.join("src/corpus.rs"))?,
        "inputsSha256":sha(&inputs)?,"unchangedPureTests":witnesses,"cases":baseline.len(),"accepted":baseline.iter().filter(|r|r["result"].get("error").is_none()).count(),"changedCases":["size-3-16777217"],"baseline":baseline,"mutation":delta});
    fs::write(args.output, json_bytes(&report)?)?;
    Ok(())
}
