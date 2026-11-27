//! Compile and compare the original and generated MissingRange contracts.
use clap::Parser;
use public_data_qualification::{
    artifacts::{self, Artifacts},
    assembly::*,
    range::*,
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FW: &str = "7855c055dc1e72b49d3654f6e6c500a0ffdb57ec";
const DARWIN_BINARY: &str = "5701bea86331ac273a213f9af52a64f5d8bf900b1b8cf7687e700bf2329a91ae";
#[derive(Parser)]
struct Args {
    input_revision: String,
    framework: PathBuf,
    original: PathBuf,
    baseline: PathBuf,
    compiler_target: PathBuf,
    work: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    darwin_compiler: Option<PathBuf>,
}
fn witness(
    original: &Path,
    host: &str,
    art: &Artifacts,
    target: &Path,
    work: &Path,
) -> Result<Value> {
    fs::create_dir(work)?;
    let role = &art.roles[ENGINE];
    let reference = whole().join("gap-range-reference");
    let record = whole().join("gap-range-record");
    let yes = imports::probe(
        host,
        role,
        work,
        "pub use observed::MissingRange as Witness;",
    )?;
    let no = imports::probe(
        host,
        role,
        work,
        "pub use observed::model::MissingRange as Witness;",
    )?;
    require(
        yes["compiles"] == true
            && no["compiles"] == false
            && list(&no["diagnostics"])?.len() == 1
            && no["diagnostics"][0]["code"] == "E0603",
        "unexpected MissingRange import visibility",
    )?;
    let lib = Path::new(text(&role[1])?);
    let compile = |src: &Path, name: &str| -> Result<PathBuf> {
        let exe = work.join(name);
        let mut c = Command::new("rustc");
        c.args([
            "+1.97.1",
            "--edition=2024",
            "--target",
            host,
            "--cfg",
            "with_range",
        ]);
        manifest_native::external(&mut c, "marmish_engine", lib);
        manifest_native::external(&mut c, "serde_json", &art.serde_json);
        for dir in [
            target.join(host).join("release/deps"),
            target.join("release/deps"),
        ] {
            manifest_native::depend(&mut c, &dir);
        }
        for link in &art.links {
            c.arg("-L").arg(link);
        }
        run(c.arg(src).arg("-o").arg(&exe))?;
        Ok(exe)
    };
    let probe = compile(&reference.join("probe.rs"), "probe")?;
    let traits = compile(&record.join("traits.rs"), "traits")?;
    let cases = reference.join("cases.json");
    let input = fs::read(&cases)?;
    let result = capture(Command::new(probe).current_dir(work), 60, Some(&input))?;
    require(result.status.success(), "paired value probe failed")?;
    let observed: Value = serde_json::from_slice(&result.stdout)?;
    let traits = stdout(Command::new(traits).current_dir(work))?;
    let fixture = reference.join(format!("original-{host}.json"));
    let original_row = load(&fixture)?;
    let source=run(Command::new("git").current_dir(root()).args(["show","2c69f8c38cbd57136eb684f5b515ef1490f017b3:verification/whole/gap-range-reference/observe.py"]))?;
    for (k, v) in [
        ("original", ORIGINAL.into()),
        ("host", host.into()),
        ("modelSha256", sha(&original.join(MODEL))?),
        ("cargoLockSha256", sha(&original.join("Cargo.lock"))?),
        ("casesSha256", sha(&cases)?),
        ("probeSha256", sha(&reference.join("probe.rs"))?),
        ("observerSha256", bytes_sha(&source)),
    ] {
        require(
            original_row[k] == v,
            format!("pinned upstream fixture differs: {k}"),
        )?;
    }
    let standalone = record.join(format!("{host}.json"));
    let ids: Vec<_> = list(&load(&cases)?)?
        .iter()
        .filter(|v| v["type"] == "range")
        .map(|v| v["id"].clone())
        .collect();
    let rows: Vec<_> = list(&observed)?
        .iter()
        .filter(|v| ids.contains(&v["id"]))
        .collect();
    require(
        ids.len() == 18
            && rows.iter().map(|v| &v["id"]).eq(ids.iter())
            && observed == original_row["results"]
            && observed == load(&standalone)?["baseline"]["observations"]
            && traits == "public fields, reversed interval, Clone and Eq\n",
        "range observations differ from independent native qualification",
    )?;
    Ok(
        json!({"publicImport":yes,"privateModuleControl":no,"rangeCases":rows,"pairedObservations":observed,"traits":traits,"originalReferenceSha256":sha(&fixture)?,"standaloneReceiptSha256":sha(&standalone)?,"engineLibrarySha256":sha(lib)?}),
    )
}
fn main() -> Result {
    let a = Args::parse();
    let framework = a.framework.canonicalize()?;
    let original = a.original.canonicalize()?;
    let base = a.baseline.canonicalize()?;
    checkout(root(), &a.input_revision, true)?;
    checkout(&framework, FW, true)?;
    checkout(&original, ORIGINAL, true)?;
    api_composition::compose(
        api_composition::Release::CanonicalRangeData,
        &portable(),
        true,
    )?;
    let (host, version) = host()?;
    let source = artifacts::source_pin(&original)?;
    let work = output_path(&a.work, &[root(), &framework, &original, &base])?;
    let exe = if let Some(p) = a.darwin_compiler {
        require(
            host == DARWIN && sha(&p)? == DARWIN_BINARY,
            "wrong reviewed Darwin compiler",
        )?;
        p.canonicalize()?
    } else {
        fs::create_dir_all(&a.compiler_target)?;
        let t = a.compiler_target.canonicalize()?;
        run(build_env(
            Command::new("cargo")
                .current_dir(&framework)
                .args([
                    "+1.97.1",
                    "build",
                    "--locked",
                    "--offline",
                    "--bin",
                    "oddlang",
                    "--target-dir",
                ])
                .arg(&t),
        ))?;
        t.join("debug/oddlang")
    };
    fs::create_dir(&work)?;
    let cli = work.join("oddlang");
    fs::copy(&exe, &cli)?;
    require(sha(&cli)? == sha(&exe)?, "staged compiler digest changed")?;
    let manifest = portable().join(format!("{AUTHORED}.json"));
    let inputs = work.join("inputs");
    let spec = stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    let cache = work.join("providers");
    let git = providers(&portable().join(text(&spec["cargoLock"]["path"])?), &cache)?;
    let temp = tempfile::tempdir()?;
    let scratch = temp.path().canonicalize()?;
    let sandbox = Sandbox::new(
        &host,
        &work,
        &[root(), &framework, &original, &base, &exe, &scratch],
        &cache,
    )?;
    let denied = [
        original.join(MODEL),
        framework.join("src/lib.rs"),
        base.join(ENGINE).join("src/lib.rs"),
        whole().join("gap-range-reference/probe.rs"),
        whole().join("gap-range-record/traits.rs"),
    ];
    sandbox.deny(&denied)?;
    let name = format!("{AUTHORED}.json");
    let product = work.join("candidate");
    let target = work.join("target");
    run(sandbox
        .command(&work)?
        .arg(&cli)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(name))
        .arg(&product))?;
    let prior_path = whole()
        .join("canonical-public-data")
        .join(format!("{host}.json"));
    let prior = load(&prior_path)?;
    let before = snapshots(&base, &product, &prior, &a.input_revision)?;
    let raw = run(sandbox
        .command(&product)?
        .args([
            "cargo",
            "build",
            "--workspace",
            "--lib",
            "--release",
            "--target",
            &host,
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&target)
        .arg("--message-format=json"))?;
    fs::write(work.join("cargo-artifacts.jsonl"), &raw)?;
    let a_set = artifacts::artifacts(&raw, &product, &target, &host)?;
    let closure = json!(a_set.files);
    let recovered = witness(&original, &host, &a_set, &target, &scratch.join("range"))?;
    let inherited = domains::observe(
        &json!({"roles":a_set.roles,"host":host,"work":scratch,"target":target,"links":a_set.links,"serde":a_set.serde,"serdeJson":a_set.serde_json}),
    )?;
    require(
        before == snapshots(&base, &product, &prior, &a.input_revision)?
            && closure == library_closure(&raw, &target, &host)?
            && source == artifacts::source_pin(&original)?,
        "sealed files, upstream source or build artifacts changed",
    )?;
    let report = json!({"format":"marmish-canonical-range-data-v1","host":host,"rustc":version,"original":source,"framework":FW,"companion":a.input_revision,"compilerSha256":sha(&cli)?,"manifestSha256":sha(&manifest)?,"priorReceiptSha256":sha(&prior_path)?,"workspace":before,"sourceDenials":denied.len(),"gitProviders":git,"stagedInputs":staged,"cargoArtifactLogSha256":bytes_sha(&raw),"cargoLibraryClosureEntries":a_set.files.len(),"cargoLibraryClosureSha256":hashed(&closure)?,"missingRange":recovered,"inheritedPublicData":inherited});
    fs::write(a.receipt, json_bytes(&report)?)?;
    Ok(())
}
