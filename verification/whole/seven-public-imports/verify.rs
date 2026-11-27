//! Qualify the seven generated libraries against the frozen native public import inventory.
use clap::Parser;
use public_data_qualification::{
    Result,
    artifacts::selected_build,
    assembly::{COMPILER, DARWIN, ORIGINAL, bytes_sha, root},
    imports, json_bytes, load,
    product::public_product_at,
    require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    product: PathBuf,
    candidate_target: PathBuf,
    original: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    release_inputs: Option<PathBuf>,
    #[arg(long)]
    candidate_artifacts: Option<PathBuf>,
    #[arg(long)]
    original_artifacts: Option<PathBuf>,
    #[arg(long)]
    candidate_profile: Option<PathBuf>,
}

fn profile(path: Option<&Path>) -> Result<Value> {
    let profile = match path {
        Some(p) => load(p)?,
        None => json!({
            "release":"workspace-v7-recovered-vault-engine-journal-mdk-buffered",
            "input_revision":"6a3eb779b60e63bef3dbfbb83e64b8a486ef77df",
            "framework_revision":"59f947dafdb4cf3055e35216857472ada895a76a",
            "lock_sha256":"fdfa5b16d9172fe97705ee5794251c6168d0e226ea159334a52286e2e7172b68"
        }),
    };
    require(
        profile.as_object().is_some_and(|p| p.len() == 4),
        "invalid candidate profile fields",
    )?;
    let release = text(&profile["release"])?;
    require(
        release.starts_with(|c: char| c.is_ascii_lowercase())
            && release
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid candidate release",
    )?;
    for (key, len) in [
        ("input_revision", 40),
        ("framework_revision", 40),
        ("lock_sha256", 64),
    ] {
        let id = text(&profile[key])?;
        require(
            id.len() == len
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid candidate pin",
        )?;
    }
    Ok(profile)
}

fn clean_command(name: &str) -> Command {
    let mut cmd = Command::new(name);
    cmd.env("CARGO_NET_OFFLINE", "true");
    for (key, _) in env::vars_os() {
        if key.to_str().is_some_and(|k| {
            [
                "RUST",
                "CARGO_BUILD_",
                "CARGO_ENCODED_",
                "CARGO_PROFILE_",
                "CARGO_TARGET_",
            ]
            .iter()
            .any(|p| k.starts_with(p))
        }) {
            cmd.env_remove(key);
        }
    }
    cmd
}

fn build(path: &Path, target: &Path, options: &[&str], archive: Option<&Path>) -> Result<Vec<u8>> {
    if let Some(p) = archive {
        return Ok(fs::read(p)?);
    }
    run(clean_command("cargo")
        .current_dir(path)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--lib",
            "--locked",
            "--offline",
        ])
        .args(options)
        .arg("--target-dir")
        .arg(target)
        .arg("--message-format=json"))
}

// Cargo accepts new target directories. Resolve their existing ancestors before checking containment.
fn resolved(path: &Path) -> Result<PathBuf> {
    if path.try_exists()? {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or("missing target parent")?;
    Ok(resolved(parent)?.join(path.file_name().ok_or("invalid target path")?))
}

fn main() -> Result {
    let args = Args::parse();
    let product = args.product.canonicalize()?;
    let original = args.original.canonicalize()?;
    let inputs = args
        .release_inputs
        .as_deref()
        .unwrap_or(root())
        .canonicalize()?;
    let candidate_target = resolved(&std::path::absolute(&args.candidate_target)?)?;
    let original_target = resolved(&std::path::absolute(&args.original_target)?)?;
    for target in [&candidate_target, &original_target] {
        require(
            !target.starts_with(&product) && !target.starts_with(&original),
            "Cargo targets must be external to source trees",
        )?;
    }
    let revision = stdout(
        Command::new("git")
            .current_dir(&original)
            .args(["rev-parse", "HEAD"]),
    )?;
    let dirty = run(Command::new("git").current_dir(&original).args([
        "status",
        "--porcelain",
        "--untracked-files=no",
    ]))?;
    require(
        revision.trim() == ORIGINAL && dirty.is_empty(),
        "expected clean tracked upstream files at pinned revision",
    )?;
    let compiler = stdout(clean_command("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        compiler
            .lines()
            .any(|l| l == format!("commit-hash: {COMPILER}"))
            && compiler.lines().any(|l| l == format!("host: {DARWIN}")),
        "wrong rustc host or commit",
    )?;
    let provenance = whole().join("public-types/provenance.json");
    require(
        load(&provenance)?["original"]["revision"] == ORIGINAL,
        "unexpected original inventory provenance",
    )?;
    let p = profile(args.candidate_profile.as_deref())?;
    let release = text(&p["release"])?;
    let check = || {
        public_product_at(
            &product,
            &inputs,
            release,
            text(&p["input_revision"])?,
            text(&p["lock_sha256"])?,
        )
    };
    let checked = check()?;
    let generated = build(
        &product,
        &candidate_target,
        &["--workspace"],
        args.candidate_artifacts.as_deref(),
    )?;
    let reference = build(
        &original,
        &original_target,
        &["--package", "marmish-prototype", "--target", DARWIN],
        args.original_artifacts.as_deref(),
    )?;
    let packages: Vec<_> = checked.0["members"]
        .as_object()
        .ok_or("missing workspace members")?
        .keys()
        .map(String::as_str)
        .collect();
    let (roles, generated_links) =
        selected_build(&generated, &product, &candidate_target, &packages)?;
    let (upstream, original_links) = selected_build(
        &reference,
        &original.join("crates"),
        &original_target,
        &["marmish-prototype"],
    )?;
    let libraries = imports::library_set(DARWIN, &original, &roles)?;
    let observations = imports::prototype(
        DARWIN,
        &json!({"original":upstream["marmish-prototype"],"generated":roles["marmish-prototype"]}),
        &json!({"original":original_links,"generated":generated_links}),
    )?;
    require(
        checked == check()?,
        "workspace changed during qualification",
    )?;
    let mut totals = json!({});
    for key in ["importable", "missing"] {
        totals[key] = imports::COUNT
            .iter()
            .map(|(name, _)| libraries[name][key].as_u64().ok_or("missing import count"))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<u64>()
            .into();
    }
    let manifest = inputs
        .join("oddlang/portable")
        .join(format!("{release}.json"));
    let report = json!({"workspaceInputRevision":p["input_revision"],"frameworkRevision":p["framework_revision"],"original":ORIGINAL,
        "workspaceLockSha256":p["lock_sha256"],"workspaceManifestSha256":sha(&manifest)?,"upstreamInventoryProvenanceSha256":sha(&provenance)?,
        "rustc":compiler.lines().collect::<Vec<_>>(),"candidateArtifactLogSha256":bytes_sha(&generated),"originalArtifactLogSha256":bytes_sha(&reference),
        "verifierSha256":sha(&env::current_exe()?)?,"imports":libraries,"totals":totals,"prototypeValueProbes":observations});
    if let Some(parent) = args.receipt.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(args.receipt, json_bytes(&report)?)?;
    Ok(())
}
