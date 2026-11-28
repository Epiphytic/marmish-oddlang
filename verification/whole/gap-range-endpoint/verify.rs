//! Verify all representation hint profiles against native and optional TypeScript callers.
use super::{MODEL, derived, qualify, sealed, write_json};
use public_data_qualification::{
    Result,
    assembly::{self, COMPILER, Sandbox},
    capture, endpoint_contract, list, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "b530e740a6eaef5e993192777463bf39e222c477";
const PROFILE: &str = "engine-gap-range-native-idioms";
const HINTS: &str = "oddlang/portable/engine-gap-range-native-idioms-v1/hints.json";
const PROFILES: [&str; 5] = [
    "baseline",
    "replacement",
    "without-rust",
    "without-typescript",
    "absent",
];

#[derive(clap::Args)]
pub(super) struct EndpointArgs {
    #[command(flatten)]
    common: derived::Args,
    #[arg(long)]
    typescript: bool,
}

fn digests(paths: &[PathBuf]) -> Result<Vec<Value>> {
    paths
        .iter()
        .map(|p| {
            if p.is_file() {
                Ok(json!(sha(p)?))
            } else {
                Ok(Value::Null)
            }
        })
        .collect()
}

fn build_command(staged: &Value, sandbox: &Sandbox, compiler: &Path) -> Result<Command> {
    let lane = Path::new(text(&staged[0])?);
    let mut command = sandbox.command(lane)?;
    command
        .arg(compiler)
        .args(["build", "native-package", "rust"])
        .arg(text(&staged[1])?)
        .arg(lane.join("product"));
    Ok(command)
}

fn private_field_control(work: &Path, sandbox: &Sandbox, compiler: &Path) -> Result<String> {
    let staged = endpoint_contract::stage(work, "negative-binding")?;
    let manifest = Path::new(text(&staged[1])?);
    let product = Path::new(text(&staged[0])?).join("product");
    let mut spec = load(manifest)?;
    let pin = &mut spec["components"][1]["overlay"];
    let overlay = manifest
        .parent()
        .ok_or("missing input directory")?
        .join(text(&pin["path"])?);
    let mut binding = load(&overlay)?;
    binding["sourceApi"]["fields"]["urn:marmish:engine:output-gap:range:end"]["visibility"] =
        json!("private");
    write_json(&overlay, binding)?;
    pin["sha256"] = json!(sha(&overlay)?);
    write_json(manifest, spec)?;
    let refused = capture(&mut build_command(&staged, sandbox, compiler)?, 600, None)?;
    let diagnostic = String::from_utf8(refused.stderr)?;
    require(
        !refused.status.success()
            && product.symlink_metadata().is_err()
            && ["unknown variant", "private", "public"]
                .iter()
                .all(|word| diagnostic.contains(word)),
        format!("unexpected private field outcome: {diagnostic}"),
    )?;
    Ok(diagnostic
        .trim()
        .replace(work.to_str().ok_or("invalid work path")?, "<work>"))
}

fn check_typescript(work: &Path, outputs: &mut Value) -> Result<(Value, Value)> {
    let ts = whole().join("gap-range-endpoint/typescript");
    let js = work.join("ts");
    let node = stdout(Command::new("node").arg("--version"))?;
    require(node.trim().starts_with("v22."), "expected Node 22")?;
    let mut hashes = json!({});
    for name in ["endpoint.ts", "check.cjs"] {
        hashes[name] = json!(sha(&ts.join(name))?);
    }
    run(Command::new("npx")
        .current_dir(&ts)
        .args([
            "--offline",
            "--package=typescript@5.9.3",
            "tsc",
            "--strict",
            "--target",
            "ES2022",
            "--module",
            "commonjs",
            "--outDir",
        ])
        .arg(&js)
        .arg("endpoint.ts"))?;
    for profile in PROFILES {
        let transcript = stdout(
            Command::new("node")
                .current_dir(work)
                .arg(ts.join("check.cjs"))
                .arg(js.join("endpoint.js"))
                .arg(work.join(profile).join("cases.json")),
        )?;
        outputs[profile]["typescript"] = json!(transcript.trim());
    }
    Ok((json!(node.trim()), hashes))
}

fn compare_contract(cases: &Value, rows: &Value) -> Result {
    let rows = list(rows)?;
    let cases = list(cases)?;
    require(
        rows.len() == cases.len(),
        "unexpected number of endpoint cases",
    )?;
    for (case, row) in cases.iter().zip(rows) {
        require(row["id"] == case["id"], "endpoint case order changed")?;
        let result = &row["result"];
        if case["expected"].is_null() {
            require(
                result.as_object().is_some_and(|o| {
                    o.len() == 3
                        && ["diagnostic", "line", "column"]
                            .iter()
                            .all(|k| o.contains_key(*k))
                }),
                "expected endpoint rejection",
            )?;
        } else {
            require(
                *result == json!({"encoded":case["expected"]}),
                "encoded endpoint bytes differ",
            )?;
        }
    }
    Ok(())
}

pub(super) fn check(args: EndpointArgs) -> Result {
    let ts = args.typescript;
    derived::run_qualification(
        args.common,
        FRAMEWORK,
        Some(HINTS),
        "baseline/product/package.lock",
        |work, args, host, builder, companion| {
            let version = qualify::version("+1.97.1", COMPILER, host)?;
            let endpoint = whole().join("gap-range-endpoint");
            let bundle = portable().join(format!("{PROFILE}-v1"));
            let paths = [
                bundle.join("wire.json"),
                endpoint.join("cases.json"),
                bundle.join("hints.json"),
                bundle.join("rust.json"),
                portable().join(format!("{PROFILE}-package.json")),
            ];
            let before = digests(&paths)?;
            let cases = endpoint_contract::validate()?;
            let compiler_dir = args.compiler_target.join("debug");
            let compiler = work.join("oddlang");
            fs::copy(compiler_dir.join("oddlang"), &compiler)?;
            let model = args.original.join(MODEL);
            let probes = [
                model.clone(),
                model.with_file_name("lib.rs"),
                whole().join("gap-range-reference/probe.rs"),
                whole().join("gap-range-record/traits.rs"),
            ];
            let cache = work.join("git-cache");
            fs::create_dir(&cache)?;
            let sandbox = Sandbox::new(
                host,
                work,
                &[
                    assembly::root(),
                    &args.original,
                    &args.framework,
                    &compiler_dir,
                ],
                &cache,
            )?;
            sandbox.deny(&probes)?;
            for profile in PROFILES {
                let staged = endpoint_contract::stage(work, profile)?;
                run(&mut build_command(&staged, &sandbox, &compiler)?)?;
            }
            let mut outputs = endpoint_contract::profiles(work)?;
            let diagnostic = private_field_control(work, &sandbox, &compiler)?;
            let (node, typescript_inputs) = if ts {
                check_typescript(work, &mut outputs)?
            } else {
                (Value::Null, json!({}))
            };
            let reference = endpoint.join(format!("{host}.json"));
            let observations = &load(&reference)?["original"]["results"];
            for profile in PROFILES {
                let lane = work.join(profile);
                let profile_cases = load(&lane.join("cases.json"))?;
                let vectors: Vec<_> = list(&profile_cases)?
                    .iter()
                    .map(|c| json!({"id":c["id"],"wire":c["wire"],"type":"range"}))
                    .collect();
                let rows = sealed::observe(
                    &sandbox,
                    &lane,
                    &serde_json::to_vec(&vectors)?,
                    observations,
                    profile,
                )?;
                compare_contract(&cases, &rows)?;
            }
            require(
                before == digests(&paths)?,
                "endpoint inputs changed during verification",
            )?;
            let verifier = sha(&env::current_exe()?)?;
            Ok(
                json!({"format":"marmish-gap-range-native-idioms-v1","framework":FRAMEWORK,"frameworkBuildToolchain":builder,"manifestSha256":before[4],"companion":companion,"compilerSha256":sha(&compiler)?,
            "node":node,"typescriptInputs":typescript_inputs,"typescript":ts.then_some("5.9.3"),"verifierSha256":verifier,"contractVerifierSha256":verifier,
            "host":host,"rustc":version,"sealedPackageLockSha256":sha(&portable().join("engine-gap-range-recovered-v1/package.lock"))?,
            "originalEndpointReceiptSha256":sha(&reference)?,"wireContractSha256":before[0],"casesSha256":before[1],"hintsSha256":before[2],"bindingSha256":before[3],
            "profiles":outputs,"privateBindingDiagnostic":diagnostic,"observations":observations,"deniedReads":probes.each_ref().map(|p|p.file_name().unwrap().to_string_lossy()),
            "wireProbeSha256":sha(&probes[2])?,"sourceApiProbeSha256":sha(&probes[3])?}),
            )
        },
    )
}
