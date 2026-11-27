//! Verify the checked Rust range binding against its qualified predecessor.
use clap::Parser;
use public_data_qualification::{api_composition, assembly::*, range::*, *};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "b530e740a6eaef5e993192777463bf39e222c477";
const RELEASE: &str = "workspace-v7-canonical-range-native-idioms";
const PACKAGE: &str = "marmish-engine-canonical-range-native-idioms-package.json";
const BINDING: &str = "engine-gap-range-native-idioms-v1/rust.json";
const OLD_OVERLAY: &str = "engine-gap-range-recovered-v1/rust.json";
const ROOT_LOCK: &str = "workspace.lock";
const ENGINE_LOCK: &str = "marmish-engine/package.lock";
const RANGE_SRC: &str = "marmish-engine/src/group_1/__projection_5.rs";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    compiler: PathBuf,
    baseline: PathBuf,
    work: PathBuf,
    receipt: PathBuf,
}

fn check_inputs(prior: &Value, staged: &Value) -> Result {
    let old = &prior["stagedInputs"];
    let base_name = format!("{RECOVERED}.json");
    let base = load(&portable().join(&base_name))?;
    let mut expected = old.clone();
    let map = expected
        .as_object_mut()
        .ok_or("missing staged input inventory")?;
    for name in [
        base_name.as_str(),
        text(&base["members"][ENGINE]["manifest"]["path"])?,
        OLD_OVERLAY,
    ] {
        require(
            map.remove(name).is_some(),
            format!("missing predecessor input {name}"),
        )?;
    }
    let release_name = format!("{RELEASE}.json");
    for name in [release_name.as_str(), PACKAGE, BINDING] {
        require(
            map.insert(name.into(), json!(sha(&portable().join(name))?))
                .is_none(),
            format!("unexpected predecessor input {name}"),
        )?;
    }
    require(
        &expected == staged,
        "unexpected sealed input changes for the role binding",
    )
}

fn compare_product(
    baseline: &Path,
    product: &Path,
    prior: &Value,
    before: &Value,
) -> Result<Value> {
    let after = tree(product)?;
    let mut common = after.as_object().ok_or("missing product tree")?.clone();
    require(keys(before)? == keys(&after)?, "emitted layout changed")?;
    let mut pins = json!({});
    for name in [ROOT_LOCK, ENGINE_LOCK] {
        common.remove(name);
        pins[name] = delta(
            &load(&baseline.join(name))?,
            &load(&product.join(name))?,
            "",
        );
    }
    for (name, row) in &common {
        require(
            *row == before[name]
                && (row["kind"] != "file"
                    || fs::read(product.join(name))? == fs::read(baseline.join(name))?),
            format!("emitted entry differs: {name}"),
        )?;
    }
    let leaf = &after[RANGE_SRC]["sha256"];
    require(
        leaf == &prior["productComparison"]["identicalRustFiles"][RANGE_SRC],
        "generated range differs from qualified Rust",
    )?;
    require(
        keys(&pins[ROOT_LOCK])?
            == [
                "inputSha256",
                "members/marmish-engine/inputSha256",
                "members/marmish-engine/standalonePackageLockSha256",
            ]
            .map(String::from)
            .into()
            && keys(&pins[ENGINE_LOCK])?
                == [
                    "manifestSha256",
                    "components/5/overlaySha256",
                    "components/5/projection/overlaySha256",
                ]
                .map(String::from)
                .into()
            && pins[ENGINE_LOCK]["components/5/overlaySha256"]["after"]
                == sha(&portable().join(BINDING))?,
        "unexpected lock provenance deltas",
    )?;
    require(
        fs::read(product.join(ROOT_LOCK))? == fs::read(portable().join(RELEASE).join(ROOT_LOCK))?,
        "assembled workspace lock differs",
    )?;
    let rows = after.as_object().ok_or("missing product tree")?;
    Ok(
        json!({"files":rows.values().filter(|r|r["kind"]=="file").count(),
        "directories":rows.values().filter(|r|r["kind"]=="directory").count(),
        "identicalRustFiles":common.keys().filter(|s|s.ends_with(".rs")).count(),
        "identicalCommonTreeSha256":hashed(&Value::Object(common))?,"rangeRustSha256":leaf,"lockDeltas":pins}),
    )
}

fn main() -> Result {
    let a = Args::parse();
    api_composition::compose(
        api_composition::Release::RangeNativeIdioms,
        &portable(),
        true,
    )?;
    let (host, version) = host()?;
    let framework = a.framework.canonicalize()?;
    let compiler = a.compiler.canonicalize()?;
    let baseline = a.baseline.canonicalize()?;
    run(Command::new("git")
        .current_dir(&framework)
        .args(["diff", "--exit-code", "HEAD"]))?;
    require(
        stdout(
            Command::new("git")
                .current_dir(&framework)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == FRAMEWORK,
        "wrong framework revision",
    )?;
    let prior_path = whole()
        .join("canonical-recovered-range")
        .join(format!("{host}.json"));
    let endpoint_path = whole()
        .join("gap-range-endpoint")
        .join(format!("{host}.json"));
    let prior = load(&prior_path)?;
    let endpoint = load(&endpoint_path)?;
    let before = tree(&baseline)?;
    require(
        hashed(&before)? == prior["productComparison"]["recoveredTreeSha256"]
            && endpoint["sourceEquivalenceReceiptSha256"] == sha(&prior_path)?
            && endpoint["generatedBuildReceiptSha256"] == prior["priorNativeReceiptSha256"]
            && endpoint["generatedWorkspaceLockSha256"] == prior["priorWorkspaceLockSha256"]
            && endpoint["casesSha256"] == sha(&endpoint_path.with_file_name("cases.json"))?,
        "recovered predecessor or native endpoint evidence differs",
    )?;
    let work = output_path(&a.work, &[root(), &framework, &baseline])?;
    fs::create_dir(&work)?;
    let inputs = work.join("inputs");
    let name = format!("{RELEASE}.json");
    let manifest = portable().join(&name);
    let spec = stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    check_inputs(&prior, &staged)?;
    let exe = work.join("oddlang");
    fs::copy(&compiler, &exe)?;
    let compiler_sha = sha(&exe)?;
    let cache = work.join("providers");
    providers(&portable().join(text(&spec["cargoLock"]["path"])?), &cache)?;
    let sandbox = Sandbox::new(
        &host,
        &work,
        &[root(), &framework, &baseline, &compiler],
        &cache,
    )?;
    sandbox.deny(&[
        portable().join(OLD_OVERLAY),
        framework.join("src/lib.rs"),
        baseline.join(ENGINE).join("src/lib.rs"),
    ])?;
    let product = work.join("candidate");
    run(sandbox
        .command(&work)?
        .arg(&exe)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(&name))
        .arg(&product))?;
    require(
        sha(&exe)? == compiler_sha,
        "compiler changed during generation",
    )?;
    let mut result = compare_product(&baseline, &product, &prior, &before)?;
    let evidence = json!({"host":host,"rustc":version,"framework":FRAMEWORK,"compilerSha256":compiler_sha,
        "manifestSha256":sha(&manifest)?,"engineManifestSha256":sha(&portable().join(PACKAGE))?,
        "roleBindingSha256":sha(&portable().join(BINDING))?,"workspaceLockSha256":sha(&product.join(ROOT_LOCK))?,
        "predecessorReceiptSha256":sha(&prior_path)?,"nativeEndpointReceiptSha256":sha(&endpoint_path)?,
        "stagedInputs":keys(&staged)?.len(),"stagedInputsSha256":hashed(&staged)?,"deniedInputs":3});
    result
        .as_object_mut()
        .unwrap()
        .extend(evidence.as_object().unwrap().clone());
    result.sort_all_objects();
    fs::write(a.receipt, json_bytes(&result)?)?;
    Ok(())
}
