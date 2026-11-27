//! Qualify the published TypeScript mode against independent ARM64 native observations.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{COMPILER, DARWIN, LINUX, bytes_sha, checkout, resolved, root},
    json_bytes, list, load, require, run, sha, stdout, text,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

const PUBLICATION: &str = "154ab6d9a085eac755acd197b2a55e53158a5999";
const WITNESS: &str = "622f2a23f3549a902d4bd96d4214ffe05330991b";
const FRAMEWORK: &str = "86b23552d02f67ee7156a4ce0ece3e07c1fc3fb0";
const DOMAIN: &str = "oddlang/portable/vault-mode-v1";
const NEGATIVE: &str = r#"
import { AuthorizationMode as Mode } from "./values";
// @ts-expect-error Ordinal numbers require the checked conversion.
const raw: Mode = 0;
// @ts-expect-error The published choice has no comparison capability.
Mode.equals(Mode.Interactive, Mode.Verify);
// @ts-expect-error The published choice does not declare partial equality.
Mode.partialEquals(Mode.Interactive, Mode.Verify);
"#;

const PROBE: &str = r#"
import { AuthorizationMode as Mode } from "./values";
const ordinals = [Mode.ordinal(Mode.Interactive), Mode.ordinal(Mode.Verify)];
const recovered = ordinals.map(value => Mode.ordinal(Mode.fromOrdinal(value)!));
if (!Object.is(ordinals[0], 0) || !Object.is(ordinals[1], 1) || !Object.is(Mode.fromOrdinal(-0), 0))
  throw new Error("exact ordinal bits differ");
const hostStrictEquality = {
  sameOrdinal: Mode.Interactive === Mode.Interactive,
  distinctOrdinals: Mode.Interactive === Mode.Verify,
};
for (const value of [2, -1, "0", null, NaN, Infinity, 0.5]) {
  if (Mode.fromOrdinal(value) !== undefined) throw new Error("invalid ordinal became a mode");
}
if ("equals" in Mode || "partialEquals" in Mode || !Object.isFrozen(Mode))
  throw new Error("mode capability or immutability changed");
console.log(JSON.stringify({ordinals, recovered, equality: Mode.equality, hostStrictEquality}));
"#;

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    compiler_target: PathBuf,
    receipt: PathBuf,
}

fn published(revision: &str, path: &str) -> Result<Vec<u8>> {
    run(Command::new("git")
        .current_dir(root())
        .args(["show", &format!("{revision}:{path}")]))
}
fn tsc() -> Command {
    let mut cmd = Command::new("npx");
    cmd.args(["--offline", "--package=typescript@5.9.3", "tsc"]);
    cmd
}

fn ordinals(receipt: &Value, host: &str, graph: &str) -> Result<[i64; 2]> {
    let baseline = &receipt["results"]["baseline"];
    require(
        receipt["host"] == host && baseline["graphSha256"] == graph,
        "graph or host differs from native witness",
    )?;
    require(
        ["Eq", "PartialEq"]
            .iter()
            .all(|k| baseline["absentTraits"][k] == "E0277"),
        "native equality trait evidence differs",
    )?;
    let mut values = BTreeMap::new();
    for line in list(&baseline["values"])? {
        let (label, value) = text(line)?
            .split_once('=')
            .ok_or("invalid native ordinal")?;
        require(
            values.insert(label, value.parse::<i64>()?).is_none(),
            "duplicate native ordinal label",
        )?;
    }
    require(values.len() == 2, "unexpected native cases")?;
    Ok([
        *values.get("interactive").ok_or("missing interactive")?,
        *values.get("verify").ok_or("missing verify")?,
    ])
}

fn main() -> Result {
    let args = Args::parse();
    let framework = args.framework.canonicalize()?;
    checkout(&framework, FRAMEWORK, false)?;
    let target = resolved(&std::path::absolute(args.compiler_target)?)?;
    require(
        !target.starts_with(&framework) && !target.starts_with(root().canonicalize()?),
        "expected external compiler target",
    )?;
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        rustc
            .lines()
            .any(|line| line == format!("commit-hash: {COMPILER}")),
        "unexpected rustc",
    )?;
    let node = stdout(Command::new("node").arg("--version"))?;
    let typescript = stdout(tsc().arg("--version"))?;
    require(
        node.trim() == "v22.22.2" && typescript.trim() == "Version 5.9.3",
        "unexpected JavaScript toolchain",
    )?;
    run(Command::new("cargo")
        .current_dir(framework)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&target))?;
    let compiler = target.join("release/oddlang");
    let graph = published(PUBLICATION, &format!("{DOMAIN}/graph.json"))?;
    let graph_sha = bytes_sha(&graph);
    let obligations = published(PUBLICATION, &format!("{DOMAIN}/obligations.json"))?;
    let mut witnesses = json!({});
    let mut native = None;
    for host in [DARWIN, LINUX] {
        let bytes = published(
            WITNESS,
            &format!("verification/whole/vault-mode/{host}.json"),
        )?;
        let values = ordinals(&serde_json::from_slice(&bytes)?, host, &graph_sha)?;
        require(
            native.replace(values).is_none_or(|v| v == values),
            "native ordinals differ by host",
        )?;
        witnesses[host] = bytes_sha(&bytes).into();
    }
    let expected = native.ok_or("missing native observations")?;
    let binding = fs::read(root().join(DOMAIN).join("typescript.json"))?;
    let scratch = tempfile::tempdir()?;
    let inputs = scratch.path().join("inputs");
    let out = scratch.path().join("output");
    fs::create_dir(&inputs)?;
    fs::write(inputs.join("graph.json"), graph)?;
    fs::write(inputs.join("typescript.json"), &binding)?;
    run(Command::new(&compiler)
        .current_dir(&inputs)
        .args([
            "build",
            "checked-values",
            "typescript",
            "graph.json",
            "typescript.json",
        ])
        .arg(&out))?;
    let lock = load(&out.join("projection.lock"))?;
    require(
        fs::read(out.join("obligations.json"))? == obligations
            && lock["sourceSha256"] == sha(&out.join("values.ts"))?
            && lock["obligationsSha256"] == bytes_sha(&obligations),
        "published obligations or projection digest differs",
    )?;
    fs::write(out.join("probe.ts"), PROBE)?;
    fs::write(out.join("negative.ts"), NEGATIVE)?;
    let flags = ["--strict", "--target", "ES2022", "--module", "commonjs"];
    run(tsc()
        .current_dir(&out)
        .args(flags)
        .args(["--noEmit", "negative.ts"]))?;
    run(tsc().current_dir(&out).args(flags).args([
        "--declaration",
        "--outDir",
        "compiled",
        "probe.ts",
    ]))?;
    let observations: Value = serde_json::from_slice(&run(
        Command::new("node").arg(out.join("compiled/probe.js"))
    )?)?;
    require(
        observations
            == json!({"ordinals":expected,"recovered":expected,"equality":"none","hostStrictEquality":{"sameOrdinal":true,"distinctOrdinals":false}}),
        "Node choice differs from native witness",
    )?;
    let receipt = json!({"publication":PUBLICATION,"framework":FRAMEWORK,"compilerSha256":sha(&compiler)?,"node":node.trim(),"typescript":typescript.trim(),
        "graphSha256":graph_sha,"verifierSha256":sha(&std::env::current_exe()?)?,"nativeWitnessRevision":WITNESS,"nativeWitnessSha256":witnesses,
        "typescriptBindingSha256":bytes_sha(&binding),"projection":lock,"declarationSha256":sha(&out.join("compiled/values.d.ts"))?,
        "observations":observations,"staticRejections":["unchecked ordinal","total equality","partial equality"]});
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
