//! Rebuild and qualify all six generated members against the original native workspace.
#[path = "../converged-public-api/product.rs"]
mod converged;
#[path = "../generated-diagnostics/verify.rs"]
mod diagnostics;
mod release;
mod source;
mod vault;
use clap::Parser;
use public_data_qualification::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};
const SIX: &str = "workspace-v7-recovered-vault-engine-journal-mdk";
const FIVE: &str = "workspace-v7-recovered-vault-engine-journal";
#[derive(Parser)]
struct Args {
    framework: PathBuf,
    original: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    compiler_target: PathBuf,
    #[arg(long)]
    original_target: PathBuf,
    #[arg(long)]
    original_bin_dir: PathBuf,
    #[arg(long)]
    framework_revision: String,
    /// Request from a workspace verifier with additional checks on the projected tree.
    #[arg(long)]
    extension: Option<PathBuf>,
    /// Build the seven-member release and run the private buffered-reader oracle.
    #[arg(long, conflicts_with = "extension")]
    buffered: bool,
    #[arg(long, conflicts_with_all = ["extension","buffered"])]
    release_manifest: bool,
    /// Qualify the offline CLI diagnostics module and its independently pinned public caller.
    #[arg(long, conflicts_with_all = ["extension", "buffered", "release_manifest"])]
    diagnostics: bool,
    /// Qualify the combined prototype and engine public API release.
    #[arg(long, conflicts_with_all = ["extension", "buffered", "release_manifest", "diagnostics"])]
    public_api: bool,
    #[arg(long, requires = "release_manifest")]
    raw_results: Option<PathBuf>,
}
#[path = "../journal-records/witness.rs"]
mod record_witness;
use record_witness::{cargo_rows, journal_probe, library};

fn mdk_check(
    cli: &Path,
    original: &Path,
    prior: &[Value],
    built: &[Value],
    work: &Path,
    scratch: &Path,
) -> Result<Value> {
    let inputs = scratch.join("mdk-source");
    let fact = mdk::recover(
        cli,
        &original.join("crates/marmish-mdk/src/lib.rs"),
        scratch,
        &inputs,
    )?;
    let archive = load(&whole().join("mdk-public-data/darwin-arm64/receipt.json"))?;
    let baseline = &archive["lanes"]["baseline"];
    require(
        fact == baseline["recovery"],
        "MDK source recovery drifted from its standalone binding",
    )?;
    for f in fs::read_dir(inputs)? {
        let f = f?;
        require(
            fs::read(f.path())?
                == fs::read(work.join("inputs/mdk-public-data-v1").join(f.file_name()))?,
            "sealed MDK member drift",
        )?;
    }
    let old = library(prior, "marmish_mdk")?;
    let serde = library(prior, "serde")?;
    let deps = serde.parent().ok_or("invalid serde library")?;
    let candidate = library(built, "marmish_mdk")?;
    let observed = mdk::observe(&old, deps, &serde, deps, &scratch.join("mdk-original"))?;
    let generated = mdk::observe(
        &candidate,
        &work.join("product/target/release/deps"),
        &serde,
        deps,
        &scratch.join("mdk-generated"),
    )?;
    require(
        observed == generated
            && bytes_sha(&generated.0) == baseline["outputSha256"]
            && generated.1 == baseline["negativeRustDiagnostics"],
        "MDK baseline differs from its independent standalone receipt",
    )?;
    Ok(json!({
        "scope": ["marmish_mdk::AdapterError","marmish_mdk::VersionObservation"],
        "standaloneCompanion": "27b0e5e7b16629cc4ec2973b729cf45e2d9c114b",
        "originalLibrarySha256": sha(&old)?,
        "sealedLibrarySha256": sha(&candidate)?,
        "recovery": fact,
        "output": String::from_utf8(generated.0)?,
        "negativeRustDiagnostics": generated.1,
    }))
}
fn execute(a: Args) -> Result {
    let framework = a.framework.canonicalize()?;
    let original = a.original.canonicalize()?;
    let originals = a.original_bin_dir.canonicalize()?;
    let extension = a.extension.as_ref().map(|f| load(f)).transpose()?;
    if let Some(request) = &extension {
        require(
            text(&request["format"])? != "marmish-six-member-mdk-public-data-v1",
            "the standalone six-member receipt requires the full pinned qualification",
        )?;
    }
    if extension.is_none() {
        checkout(&framework, &a.framework_revision, false)?;
        checkout(&original, ORIGINAL, false)?;
    }
    let rev = stdout(
        Command::new("git")
            .current_dir(root())
            .args(["rev-parse", "HEAD"]),
    )?;
    if extension.is_none() {
        checkout(root(), rev.trim(), false)?;
    }
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing host")?;
    require(
        [DARWIN, LINUX].contains(&host),
        "unsupported native qualification host",
    )?;
    if a.public_api {
        let profile = load(&whole().join("converged-public-api/import-profile.json"))?;
        require(
            profile["framework_revision"] == a.framework_revision,
            "unexpected public API compiler revision",
        )?;
        api_composition::compose(api_composition::Release::Converged, &portable(), true)?;
    } else if a.release_manifest {
        api_composition::compose(api_composition::Release::ReleaseManifest, &portable(), true)?;
    } else if a.buffered {
        buffered::compose(&portable(), true)?;
    } else if extension.is_none() && !a.diagnostics {
        six_composition::compose(&portable(), true)?;
    }
    let tmp = tempfile::tempdir()?;
    let work = match &extension {
        Some(e) => Path::new(text(&e["work"])?),
        None => tmp.path(),
    }
    .canonicalize()?;
    let temp = tempfile::tempdir()?;
    let scratch = match &extension {
        Some(e) => Path::new(text(&e["scratch"])?),
        None => temp.path(),
    }
    .canonicalize()?;
    if let Some(e) = &extension {
        require(
            e["host"] == host,
            "extension host differs from native compiler",
        )?;
    }
    fs::create_dir_all(&a.compiler_target)?;
    fs::create_dir_all(&a.original_target)?;
    let ct = a.compiler_target.canonicalize()?;
    let ot = a.original_target.canonicalize()?;
    require(
        a.framework_revision.len() == 40
            && a.framework_revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "expected a full framework commit",
    )?;
    let build_target = ct.join(&a.framework_revision);
    run(build_env(
        Command::new("cargo")
            .current_dir(&framework)
            .args([
                "+1.97.1",
                "build",
                "--offline",
                "--locked",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(&build_target),
    ))?;
    let cli = work.join("oddlang");
    fs::copy(build_target.join("debug/oddlang"), &cli)?;
    let (manifest, base, member, format) = if let Some(e) = &extension {
        (
            PathBuf::from(text(&e["manifest"])?),
            portable().join(text(&e["base"])?).with_extension(""),
            text(&e["member"])?,
            text(&e["format"])?,
        )
    } else {
        let (release, parent, member, format) = if a.public_api {
            (
                converged::RELEASE,
                converged::ERRORS,
                converged::MEMBER,
                "marmish-converged-public-api-v1",
            )
        } else if a.diagnostics {
            (
                diagnostics::RELEASE,
                diagnostics::BASE,
                diagnostics::MEMBER,
                "marmish-diagnostic-workspace-v1",
            )
        } else if a.release_manifest {
            (
                manifest_product::OUTPUT,
                manifest_product::BASE,
                manifest_product::MEMBER,
                "marmish-converged-pure-release-manifest-v1",
            )
        } else if a.buffered {
            (
                buffered::RELEASE,
                SIX,
                buffered::MEMBER,
                "marmish-seven-member-private-buffer-v1",
            )
        } else {
            (
                SIX,
                FIVE,
                "marmish-mdk",
                "marmish-six-member-mdk-public-data-v1",
            )
        };
        (
            portable().join(format!("{release}.json")),
            portable().join(parent),
            member,
            format,
        )
    };
    let inputs = work.join("inputs");
    let spec = stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    let cache = work.join("providers");
    let git = providers(&portable().join(text(&spec["cargoLock"]["path"])?), &cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[
            root(),
            &original,
            &framework,
            &ot,
            &originals,
            &ct,
            &scratch,
        ],
        &cache,
    )?;
    let denied = BTreeMap::from([
        (
            "original-journal",
            original.join("crates/marmish-native/src/journal.rs"),
        ),
        (
            "original-vault",
            original.join("crates/marmish-vault/src/lib.rs"),
        ),
        (
            "original-report",
            original.join("crates/marmish-native/src/native_manifest.rs"),
        ),
        (
            "original-mdk",
            original.join("crates/marmish-mdk/src/lib.rs"),
        ),
        ("reference-executable", originals.join("marmish")),
        ("framework-source", framework.join("src/lib.rs")),
        ("journal-caller", whole().join("journal-records/probe.rs")),
        ("mdk-caller", whole().join("mdk-public-data/probe.rs")),
        (
            "vault-holdout",
            whole().join("unified-recovered-vault/fixed_bytes.rs"),
        ),
        ("release-observer", whole().join("unified-mdk/release.rs")),
    ]);
    let mut denied: BTreeMap<String, PathBuf> =
        denied.into_iter().map(|(k, p)| (k.into(), p)).collect();
    if let Some(e) = &extension {
        for (k, p) in e["extraDenied"]
            .as_object()
            .ok_or("expected additional source-denial paths")?
        {
            denied.insert(k.clone(), PathBuf::from(text(p)?));
        }
    }
    if a.diagnostics {
        denied.insert(
            "diagnostic-source".into(),
            original.join(diagnostics::SOURCE),
        );
        denied.insert(
            "diagnostic-consumer".into(),
            whole().join(diagnostics::PROBE),
        );
    }
    if a.buffered {
        denied.insert("original-prototype".into(), original.join(buffered::SOURCE));
        denied.insert(
            "buffer-caller".into(),
            whole().join("private-buffer/observations.rs"),
        );
    }
    if a.public_api || a.release_manifest {
        for (label, path) in [
            (
                "original-prototype",
                original.join("crates/marmish-prototype/src/lib.rs"),
            ),
            (
                "constant-caller",
                whole().join("prototype-constant/caller.rs"),
            ),
            (
                "reader-caller",
                whole().join("private-buffer/observations.rs"),
            ),
            (
                "original-engine-error",
                original.join("crates/marmish-engine/src/model.rs"),
            ),
            (
                "engine-error-caller",
                whole().join("engine-public-error/probe.rs"),
            ),
            (
                "original-native-choice",
                original.join("crates/marmish-native/src/identity.rs"),
            ),
            ("choice-caller", whole().join("backend-choice/probe.rs")),
        ] {
            denied.insert(label.to_owned(), path);
        }
    }
    if a.release_manifest {
        for (label, path) in [
            (
                "original-release-manifest",
                original.join(manifest_product::SOURCE),
            ),
            (
                "original-release-versions",
                original.join(manifest_product::VECTOR),
            ),
            (
                "manifest-oracle-caller",
                whole().join("release-manifest/probe.rs"),
            ),
            (
                "manifest-blind-vectors",
                whole().join("release-manifest/vectors.json"),
            ),
            (
                "manifest-original-receipt",
                whole()
                    .join("release-manifest")
                    .join(format!("{host}.json")),
            ),
        ] {
            denied.insert(label.to_owned(), path);
        }
    }
    sandbox.deny(&denied.values().cloned().collect::<Vec<_>>())?;
    sandbox.deny(&[whole().join("unified-mdk/release.rs")])?;
    let product = work.join("product");
    run(sandbox
        .command(&work)?
        .arg(&cli)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(manifest.file_name().ok_or("missing manifest name")?))
        .arg(&product))?;
    let lock = if extension.is_some() {
        // The parent verifies its additional package locks before this process builds any code.
        println!("check-product");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let checked: Value = serde_json::from_str(&line)?;
        require(
            checked == load(&product.join("workspace.lock"))?,
            "extension returned a different workspace lock",
        )?;
        product::check_members(&product, &checked, false)?;
        checked
    } else if a.public_api {
        converged::check(&product)?
    } else if a.diagnostics {
        diagnostics::check(&product)?
    } else if a.release_manifest {
        manifest_product::check(&product)?
    } else if a.buffered {
        buffered::check(&product)?
    } else {
        product::six(&product)?
    };
    let raw = run(sandbox.command(&product)?.args([
        "cargo",
        "build",
        "--workspace",
        "--release",
        "--locked",
        "--offline",
        "--message-format=json",
    ]))?;
    if extension.is_some() {
        fs::write(scratch.join("workspace-cargo-artifacts.jsonl"), &raw)?;
    }
    let build = cargo_rows(&raw)?;
    let collect = |name: &str| -> Result<Vec<Value>> {
        cargo_rows(&run(build_env(
            Command::new("cargo")
                .current_dir(&original)
                .args([
                    "+1.97.1",
                    "build",
                    "--release",
                    "--offline",
                    "--locked",
                    "--lib",
                    "-p",
                    name,
                    "--message-format=json",
                    "--target-dir",
                ])
                .arg(&ot),
        ))?)
    };
    let report = source::report(&cli, &original, &inputs, &spec, &scratch)?;
    let mut journal = source::journal(&cli, &original, &scratch)?;
    let release = release::observe(
        &original,
        &portable(),
        &originals,
        &product.join("target/release"),
        host,
    )?;
    let vault = vault::observe(
        &original,
        &ot,
        host,
        (&work, &scratch),
        &library(&build, "marmish_vault")?,
        &spec,
        extension
            .as_ref()
            .and_then(|e| e["fixedVaultPredecessor"].as_str())
            .map(Path::new),
    )?;
    let old = journal_probe(
        &scratch.join("journal-original"),
        &collect("marmish-native")?,
    )?;
    let new = journal_probe(&scratch.join("journal-generated"), &build)?;
    let expected = json!([
        r#"{"session":"s\n1","group_id":"group","peer":"北","revoked":true}"#,
        r#"{"application_id":"app","app_event_id":"event","state":"sent"}"#,
        r#"{"cursor":18446744073709551615,"session":"s\n1","application_id":"app","sender":"北","data":[0,127,255],"invalidated":false}"#
    ]);
    require(
        old["jsonLines"] == new["jsonLines"] && new["jsonLines"] == expected,
        "sealed native journal public records differ",
    )?;
    journal["original"] = old;
    journal["generated"] = new;
    let mdk = mdk_check(
        &cli,
        &original,
        &collect("marmish-mdk")?,
        &build,
        &work,
        &scratch,
    )?;
    let members: BTreeSet<_> = lock["members"]
        .as_object()
        .ok_or("missing workspace members")?
        .keys()
        .cloned()
        .collect();
    let unchanged: Vec<_> = members.iter().filter(|m| *m != member).collect();
    let mut result = json!({
        "format": format,
        "platform": host,
        "framework": a.framework_revision,
        "original": ORIGINAL,
        "companion": rev.trim(),
        "predecessorWorkspaceLockSha256": sha(&base.join("workspace.lock"))?,
        "compilerSha256": sha(&cli)?,
        "workspaceManifestSha256": sha(&manifest)?,
        "workspaceLockSha256": sha(&product.join("workspace.lock"))?,
        "cargoLockSha256": lock["cargoLockSha256"],
        "physicalMembers": members,
        "unchangedMemberLocks": unchanged,
        "stagedInputs": staged,
        "deniedSourceProbes": denied.keys().collect::<Vec<_>>(),
        "gitProviders": git,
        "recoveredReport": report,
        "releaseInfo": release,
        "fixedVault": vault,
        "journal": journal,
        "mdk": mdk,
    });
    if a.diagnostics {
        result["diagnostics"] = diagnostics::observe(&product, &original, &cli, &scratch, host)?;
    }
    if a.buffered {
        result["bufferedInput"] = buffered::observe(&product, &original, host, None)?;
    }
    if a.public_api || a.release_manifest {
        let parser = manifest_native::ParserRun {
            product: &product,
            original: &original,
            original_target: &ot,
            scratch: &scratch,
            host,
            build: &build,
        };
        for (key, value) in parser
            .inherited(&work, &cli)?
            .as_object()
            .ok_or("invalid inherited observations")?
        {
            result[key] = value.clone();
        }
        if a.public_api {
            result["parentWorkspaceLocks"] = converged::parent_hashes()?;
        } else {
            result["releaseManifest"] = parser.observe(a.raw_results.as_deref())?;
        }
    }
    if extension.is_some() {
        result["format"] = "marmish-extended-workspace-intermediate-v1".into();
        result["requestedFormat"] = format.into();
    }
    fs::write(a.receipt, json_bytes(&result)?)?;
    Ok(())
}
fn main() -> Result {
    execute(Args::parse())
}
