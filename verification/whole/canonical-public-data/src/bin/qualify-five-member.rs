//! Rebuild the five-member public release-info, vault and journal qualification.
use clap::Parser;
use public_data_qualification::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[path = "../../../unified-mdk/release.rs"]
mod release;
#[path = "../../../unified-mdk/source.rs"]
mod source;
#[path = "../../../unified-mdk/vault.rs"]
mod vault;
#[path = "../../../journal-records/witness.rs"]
mod witness;

const FRAMEWORK: &str = "84b173b288ef41b60c87aa45291f3f20ccb44bed";
const BEFORE: &str = "workspace-v7-recovered-vault-engine";
const FIVE: &str = "workspace-v7-recovered-vault-engine-journal";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    original: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    original_target: PathBuf,
    #[arg(long)]
    original_bin_dir: PathBuf,
    #[arg(long)]
    compiler_target: Option<PathBuf>,
}

fn composition(inputs: &Path) -> Result {
    let mut base = load(&inputs.join(format!("{BEFORE}.json")))?;
    let new = load(&inputs.join(format!("{FIVE}.json")))?;
    let old_native = &mut base["members"]["marmish-native"]["manifest"];
    let new_native = &new["members"]["marmish-native"]["manifest"];
    let mut desired = load(&inputs.join(text(&old_native["path"])?))?;
    let thin = load(&inputs.join("native-journal-records-package.json"))?;
    let [journal] = list(&thin["components"])?.as_slice() else {
        return Err("expected one journal component".into());
    };
    desired["components"]
        .as_array_mut()
        .ok_or("missing native components")?
        .push(journal.clone());
    require(
        desired == load(&inputs.join(text(&new_native["path"])?))?,
        "combined native input differs beyond the public records",
    )?;
    *old_native = new_native.clone();
    require(
        base == new,
        "combined workspace changed the Cargo pin, siblings or edges",
    )
}
fn check_product(product: &Path) -> Result<Value> {
    let path = product.join("workspace.lock");
    require(
        fs::read(&path)? == fs::read(portable().join(FIVE).join("workspace.lock"))?,
        "combined workspace lock drift",
    )?;
    let lock = load(&path)?;
    let previous = load(&portable().join(BEFORE).join("workspace.lock"))?;
    for key in ["workspaceCargoManifestSha256", "cargoLockSha256"] {
        require(
            lock[key] == previous[key],
            format!("workspace changed {key}"),
        )?;
    }
    let old = previous["members"]
        .as_object()
        .ok_or("missing predecessor members")?;
    let new = lock["members"].as_object().ok_or("missing members")?;
    require(old.len() == new.len(), "member inventory changed")?;
    for (name, known) in old {
        let row = new.get(name).ok_or("missing workspace member")?;
        if name == "marmish-native" {
            for key in ["dependencies", "workspaceCargoManifestSha256"] {
                require(
                    row[key] == known[key],
                    format!("native member changed {key}"),
                )?;
            }
        } else {
            require(row == known, format!("member changed: {name}"))?;
        }
    }
    product::check_members(product, &lock, false)?;
    Ok(lock)
}
fn main() -> Result {
    let args = Args::parse();
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let bins = args.original_bin_dir.canonicalize()?;
    checkout(&framework, FRAMEWORK, false)?;
    checkout(&original, ORIGINAL, false)?;
    let revision = stdout(
        Command::new("git")
            .current_dir(root())
            .args(["rev-parse", "HEAD"]),
    )?;
    checkout(root(), revision.trim(), false)?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        version.lines().any(|l| l == format!("host: {DARWIN}")),
        "expected native Darwin ARM64",
    )?;
    let portable = portable();
    composition(&portable)?;
    let manifest = portable.join(format!("{FIVE}.json"));
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let outside = tempfile::tempdir()?;
    let scratch = outside.path().canonicalize()?;
    fs::create_dir_all(&args.original_target)?;
    let target = args.original_target.canonicalize()?;
    let compiler_target = args
        .compiler_target
        .unwrap_or_else(|| scratch.join("compiler"));
    fs::create_dir_all(&compiler_target)?;
    let ct = compiler_target.canonicalize()?;
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
            .arg(&ct),
    ))?;
    let cli = work.join("oddlang");
    fs::copy(ct.join("debug/oddlang"), &cli)?;
    let inputs = work.join("inputs");
    let spec = stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    let cache = work.join("providers");
    let git = providers(&portable.join(text(&spec["cargoLock"]["path"])?), &cache)?;
    let sandbox = Sandbox::new(
        DARWIN,
        &work,
        &[root(), &original, &framework, &target, &bins, &scratch, &ct],
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
        ("reference-executable", bins.join("marmish")),
        ("framework-source", framework.join("src/lib.rs")),
        ("journal-caller", whole().join("journal-records/probe.rs")),
        (
            "vault-holdout",
            whole().join("unified-recovered-vault/fixed_bytes.rs"),
        ),
        ("release-observer", whole().join("unified-mdk/release.rs")),
    ]);
    sandbox.deny(&denied.values().cloned().collect::<Vec<_>>())?;
    let product = work.join("product");
    run(sandbox
        .command(&work)?
        .arg(&cli)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(format!("{FIVE}.json")))
        .arg(&product))?;
    let lock = check_product(&product)?;
    let artifacts = witness::cargo_rows(&run(sandbox.command(&product)?.args([
        "cargo",
        "build",
        "--workspace",
        "--release",
        "--locked",
        "--offline",
        "--message-format=json",
    ]))?)?;
    let report = source::report(&cli, &original, &inputs, &spec, &scratch)?;
    let mut journal = source::journal(&cli, &original, &scratch)?;
    let release = release::observe(
        &original,
        &portable,
        &bins,
        &product.join("target/release"),
        DARWIN,
    )?;
    let fixed = vault::observe(
        &original,
        &target,
        DARWIN,
        (&work, &scratch),
        &witness::library(&artifacts, "marmish_vault")?,
        &spec,
        None,
    )?;
    let original_build = witness::cargo_rows(&run(build_env(
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
                "marmish-native",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(&target),
    ))?)?;
    let observed =
        witness::journal_probe(&scratch.join("original-journal-probe"), &original_build)?;
    let candidate = witness::journal_probe(&scratch.join("generated-journal-probe"), &artifacts)?;
    let standalone = load(&whole().join("journal-records/aarch64-apple-darwin.json"))?;
    require(
        observed["jsonLines"] == candidate["jsonLines"]
            && candidate["jsonLines"] == standalone["generated"]["baseline"]["jsonLines"],
        "journal field order differs from original or archived independent witness",
    )?;
    journal["original"] = observed;
    journal["generated"] = candidate;
    let receipt = json!({"format":"marmish-unified-recovered-vault-journal-v1","platform":DARWIN,"framework":FRAMEWORK,"original":ORIGINAL,"companion":revision.trim(),
        "recoveredVaultAncestor":"e54db134a2cf649c5cffcf3822f4e98b3439f141","journalAncestor":"380043bf9e7686e24cc00436486396ef4308edfb",
        "compilerSha256":sha(&cli)?,"workspaceManifestSha256":sha(&manifest)?,"workspaceLockSha256":sha(&product.join("workspace.lock"))?,"cargoLockSha256":lock["cargoLockSha256"],
        "unchangedMemberLocks":["marmish-core","marmish-engine","marmish-stream","marmish-vault"],"stagedInputs":staged,"deniedSourceProbes":denied.keys().collect::<Vec<_>>(),"gitProviders":git,"releaseInfo":release,"recoveredReport":report,"fixedVault":fixed,"journal":journal});
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
