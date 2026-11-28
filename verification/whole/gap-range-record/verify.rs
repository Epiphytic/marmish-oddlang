//! Qualify the authored range package against an independently compiled upstream mutation.
use super::{MODEL, qualify, sealed, witnesses, write_json};
use public_data_qualification::{
    Result,
    assembly::{self, COMPILER, ORIGINAL, Sandbox},
    gap_callers, list, load, pin, portable, require, run, sha, stdout, whole,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const SCOPE: &str = "engine-gap-range-v1";
const PACKAGE: &str = "engine-gap-range-package.json";

#[derive(clap::Args)]
pub(super) struct RecordArgs {
    framework: PathBuf,
    compiler_target: PathBuf,
    original: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
}

fn compose(inputs: &Path, policy: &str) -> Result<PathBuf> {
    let base = portable();
    let mut spec = load(&base.join("engine-gap-reason-package.json"))?;
    assembly::pinned_copy(&base, &spec["cargoLock"], inputs)?;
    for component in list(&spec["components"])? {
        for key in ["graph", "overlay"] {
            assembly::pinned_copy(&base, &component[key], inputs)?;
        }
    }
    let record = inputs.join(SCOPE);
    fs::create_dir(&record)?;
    let graph = record.join("graph.json");
    let original = base.join(SCOPE).join("graph.json");
    let mut value = load(&original)?;
    let field = value
        .pointer_mut("/modules/0/records/0/readPolicy/unknownFields")
        .ok_or("missing read policy")?;
    if *field == policy {
        fs::copy(&original, &graph)?;
    } else {
        *field = json!(policy);
        write_json(&graph, value)?;
    }
    fs::copy(base.join(SCOPE).join("rust.json"), record.join("rust.json"))?;
    spec["components"].as_array_mut().ok_or("missing package components")?.push(json!({"kind":"public-records","module":"model","reexports":["MissingRange"],
        "graph":pin(inputs,&format!("{SCOPE}/graph.json"))?, "overlay":pin(inputs,&format!("{SCOPE}/rust.json"))?}));
    let manifest = inputs.join(PACKAGE);
    write_json(&manifest, spec)?;
    Ok(manifest)
}

pub(super) fn check(args: RecordArgs) -> Result {
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let original_target = assembly::resolved(&std::path::absolute(args.original_target)?)?;
    let compiler_target = assembly::resolved(&std::path::absolute(args.compiler_target)?)?;
    assembly::checkout(&framework, &args.framework_revision, false)?;
    assembly::checkout(&original, ORIGINAL, false)?;
    let companion = stdout(
        Command::new("git")
            .current_dir(assembly::root())
            .args(["rev-parse", "HEAD"]),
    )?;
    assembly::checkout(assembly::root(), companion.trim(), false)?;
    let host = qualify::host()?;
    let rustc = qualify::version("+1.97.1", COMPILER, host)?;
    let reference = witnesses::original_reference(&original, host)?;
    let baseline = &reference["results"];
    let fixture = whole().join("gap-range-reference");
    let cases = fs::read(fixture.join("cases.json"))?;
    run(qualify::tool("cargo")
        .current_dir(&framework)
        .args([
            "+1.96.1",
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    let tmp = tempfile::tempdir_in("/tmp")?;
    let scratch = tmp.path().canonicalize()?;
    let mutant = scratch.join("original-mutant");
    let source = qualify::mutate(&original, &mutant)?;
    let mutation_sha = sha(&source)?;
    let (observed, libraries) =
        gap_callers::mutated_range(&mutant, &original_target, host, &cases)?;
    require(
        sha(&source)? == mutation_sha
            && run(Command::new("git").current_dir(&mutant).args([
                "status",
                "--porcelain",
                "--untracked-files=all",
            ]))? == format!(" M {MODEL}\n").as_bytes(),
        "controlled original source mutation changed",
    )?;
    witnesses::check_observations(
        list(&serde_json::from_slice(&cases)?)?,
        list(baseline)?,
        list(&observed)?,
    )?;

    let isolated = tempfile::tempdir()?;
    let work = isolated.path().canonicalize()?;
    let compiler = work.join("oddlang");
    fs::copy(compiler_target.join("debug/oddlang"), &compiler)?;
    let compiler_sha = sha(&compiler)?;
    let probes = [
        original.join(MODEL),
        source,
        fixture.join("probe.rs"),
        whole().join("gap-range-record/traits.rs"),
    ];
    let cache = work.join("empty-git-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[assembly::root(), &original, &scratch, &compiler_target],
        &cache,
    )?;
    sandbox.deny(&probes)?;
    let mut products = json!({});
    for (label, policy, expected) in [
        ("baseline", "ignore", baseline),
        ("mutation", "reject", &observed),
    ] {
        let lane = work.join(label);
        let inputs = lane.join("inputs");
        fs::create_dir_all(&inputs)?;
        let manifest = compose(&inputs, policy)?;
        if label == "baseline" {
            require(
                fs::read(&manifest)? == fs::read(portable().join(PACKAGE))?,
                "baseline input manifest changed",
            )?;
        }
        let product = lane.join("product");
        run(sandbox
            .command(&lane)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&manifest)
            .arg(&product))?;
        let lock = product.join("package.lock");
        if label == "baseline" {
            require(
                fs::read(&lock)? == fs::read(portable().join(SCOPE).join("package.lock"))?,
                "baseline package lock changed",
            )?;
        }
        sealed::generated_sources(&product)?;
        run(sandbox.command(&product)?.args([
            "cargo",
            "build",
            "--locked",
            "--offline",
            "--lib",
            "--release",
        ]))?;
        let rows = sealed::observe(&sandbox, &lane, &cases, expected, label)?;
        products[label] =
            json!({"unknownFields":policy,"packageLockSha256":sha(&lock)?,"observations":rows});
    }
    write_json(
        &args.receipt,
        json!({"format":"marmish-public-gap-range-v1","original":ORIGINAL,
        "framework":args.framework_revision,"compilerSha256":compiler_sha,"companion":companion.trim(),
        "rustc":rustc,"originalReferenceSha256":sha(&fixture.join(format!("original-{host}.json")))?,"sourceDenials":probes.len(),
        "nativeTraitsProbeSha256":sha(&probes[3])?,"sourceModelSha256":sha(&probes[0])?,
        "unchangedProbeSha256":sha(&probes[2])?,"casesSha256":sha(&fixture.join("cases.json"))?,
        "mutationSourceModelSha256":mutation_sha,"mutationLibrarySha256":libraries,"changedRows":["unknown-field"],"baseline":products["baseline"],"mutation":products["mutation"]}),
    )
}
