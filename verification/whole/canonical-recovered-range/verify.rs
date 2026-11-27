//! Prove equivalence of the source-recovered release to the archived native build.
use clap::Parser;
use public_data_qualification::{assembly::*, range::*, *};
use serde_json::json;
use std::{fs, path::PathBuf};
const FW: &str = "9923e6664899de66915ed3a94e417f261ffed401";
const REASON: &str = "engine-gap-reason-recovered-v1";
const RANGE: &str = "engine-gap-range-recovered-v1";
#[derive(Parser)]
struct Args {
    input_revision: String,
    framework: PathBuf,
    compiler: PathBuf,
    original: PathBuf,
    prior_work: PathBuf,
    work: PathBuf,
    receipt: PathBuf,
}
fn main() -> Result {
    let a = Args::parse();
    let framework = a.framework.canonicalize()?;
    let original = a.original.canonicalize()?;
    let prior_work = a.prior_work.canonicalize()?;
    let compiler = a.compiler.canonicalize()?;
    checkout(root(), &a.input_revision, true)?;
    checkout(&framework, FW, true)?;
    checkout(&original, ORIGINAL, true)?;
    api_composition::compose(
        api_composition::Release::CanonicalRecoveredRange,
        &portable(),
        true,
    )?;
    let (host, version) = host()?;
    let recovery_path = whole()
        .join("gap-range-recovery")
        .join(&host)
        .join("receipt.json");
    let recovery = load(&recovery_path)?;
    require(
        recovery["framework"] == FW
            && recovery["original"] == ORIGINAL
            && recovery["compilerSha256"] == sha(&compiler)?,
        "recovered compiler or original pin differs",
    )?;
    let reason_path = whole()
        .join("gap-reason-recovery")
        .join(format!("{host}.json"));
    let reason = load(&reason_path)?;
    let range = &recovery["cases"]["baseline"];
    let model = original.join(MODEL);
    require(
        reason["original"] == ORIGINAL
            && reason["baselineRecovery"]["sourceSha256"] == sha(&model)?
            && recovery["crateRootSha256"] == sha(&model.with_file_name("lib.rs"))?,
        "recovery receipt provenance differs from original",
    )?;
    for (folder, e) in [(REASON, &reason["baselineRecovery"]), (RANGE, range)] {
        for (key, f) in [
            ("graphSha256", "graph.json"),
            ("overlaySha256", "rust.json"),
        ] {
            require(
                sha(&portable().join(folder).join(f))? == e[key],
                "recovered graph or binding differs",
            )?;
        }
    }
    let prior_path = whole()
        .join("canonical-range-data")
        .join(format!("{host}.json"));
    let prior = load(&prior_path)?;
    require(
        range["observations"] == prior["missingRange"]["pairedObservations"],
        "upstream and retained native observations differ",
    )?;
    let old_hash = text(&prior["workspace"]["workspaceLockSha256"])?;
    let baseline = prior_work.join("candidate");
    let target = prior_work.join("target");
    product::public_product(&baseline, AUTHORED, text(&prior["companion"])?, old_hash)?;
    let raw = fs::read(prior_work.join("cargo-artifacts.jsonl"))?;
    require(
        bytes_sha(&raw) == prior["cargoArtifactLogSha256"],
        "wrong archived Cargo artifact log",
    )?;
    let libraries = library_closure(&raw, &target, &host)?;
    let count = keys(&libraries)?.len();
    let closure_hash = hashed(&libraries)?;
    require(
        prior["cargoLibraryClosureEntries"] == count
            && prior["cargoLibraryClosureSha256"] == closure_hash,
        "retained native build closure differs",
    )?;
    let work = workdir(&a.work, &[root(), &original, &framework, &prior_work])?;
    let inputs = work.join("inputs");
    let manifest = portable().join(format!("{RECOVERED}.json"));
    let spec = stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    let fallback = [
        "engine-gap-reason-v1",
        "engine-gap-range-v1",
        "engine-gap-reason-package.json",
        "engine-gap-range-package.json",
    ];
    for f in keys(&staged)? {
        require(
            !fallback.contains(&f.split('/').next().unwrap_or("")),
            "authored fallback in staged inputs",
        )?;
    }
    for folder in [REASON, RANGE] {
        for file in ["graph.json", "rust.json"] {
            require(
                staged.get(format!("{folder}/{file}")).is_some(),
                "missing recovered declaration",
            )?;
        }
    }
    let exe = work.join("oddlang");
    fs::copy(&compiler, &exe)?;
    require(
        sha(&exe)? == recovery["compilerSha256"],
        "staged recovery compiler changed",
    )?;
    let cache = work.join("providers");
    let git = providers(&portable().join(text(&spec["cargoLock"]["path"])?), &cache)?;
    let tmp = tempfile::tempdir()?;
    let scratch = tmp.path().canonicalize()?;
    let sandbox = Sandbox::new(
        &host,
        &work,
        &[
            root(),
            &original,
            &framework,
            &prior_work,
            &compiler,
            &scratch,
        ],
        &cache,
    )?;
    let mut denied = std::collections::BTreeMap::from([
        ("original-model".to_string(), model),
        ("framework-source".into(), framework.join("src/lib.rs")),
        (
            "baseline-engine".into(),
            baseline.join(ENGINE).join("src/lib.rs"),
        ),
        (
            "independent-range-caller".into(),
            whole().join("gap-range-reference/probe.rs"),
        ),
    ]);
    for f in ["reason", "range"] {
        for (k, name) in [("graph", "graph.json"), ("overlay", "rust.json")] {
            denied.insert(
                format!("authored-missing-{f}-{k}"),
                portable().join(format!("engine-gap-{f}-v1")).join(name),
            );
        }
    }
    sandbox.deny(&denied.values().cloned().collect::<Vec<_>>())?;
    let candidate = work.join("candidate");
    run(sandbox
        .command(&work)?
        .arg(&exe)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(format!("{RECOVERED}.json")))
        .arg(&candidate))?;
    let hash = sha(&portable().join(RECOVERED).join("workspace.lock"))?;
    product::public_product(&candidate, RECOVERED, &a.input_revision, &hash)?;
    let compared = compare(&baseline, &candidate)?;
    require(
        compared["identicalRustFiles"]["marmish-engine/src/group_1/__projection_5.rs"]
            == range["generatedFiles"]["src/group_0/__projection_1.rs"],
        "combined range leaf differs from independently recovered code",
    )?;
    require(
        libraries == library_closure(&raw, &target, &host)?,
        "retained native build closure changed",
    )?;
    let receipt = json!({"format":"marmish-canonical-recovered-range-equivalence-v1","host":host,"rustc":version,"framework":FW,"compilerSha256":sha(&exe)?,"companion":a.input_revision,"original":ORIGINAL,"manifestSha256":sha(&manifest)?,"workspaceLockSha256":hash,"priorWorkspaceLockSha256":old_hash,"rangeRecoveryReceiptSha256":sha(&recovery_path)?,"reasonRecoveryReceiptSha256":sha(&reason_path)?,"priorNativeReceiptSha256":sha(&prior_path)?,"priorCargoArtifactLogSha256":prior["cargoArtifactLogSha256"],"priorCargoLibraryClosureEntries":count,"priorCargoLibraryClosureSha256":closure_hash,"sourceDenials":denied.keys().collect::<Vec<_>>(),"gitProviders":git,"stagedInputs":staged,"productComparison":compared});
    fs::write(a.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
