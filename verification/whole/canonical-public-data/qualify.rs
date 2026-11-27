//! Reconstruct both workspaces, isolate the native build, and observe its four public declarations.
use clap::Parser;
use public_data_qualification::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FRAMEWORK: &str = "900cfa517d0136fa80c2b5e92ccf9b1a3b2a71ab";
const BASE: &str = "workspace-v7-canonical-offline";
const RELEASE: &str = "workspace-v7-canonical-public-data";
const PKGS: [&str; 7] = [
    "core",
    "engine",
    "mdk",
    "native",
    "prototype",
    "stream",
    "vault",
];
const BASE_PIN: &str = "0f9e57396530fa1c11127e3fff7d8d0018514f00";
fn source_pin(original: &Path) -> Result<Value> {
    checkout(original, ORIGINAL, false)?;
    let tree = stdout(
        Command::new("git")
            .current_dir(original)
            .args(["rev-parse", "HEAD^{tree}"]),
    )?;
    let mut files = json!({});
    for p in ["Cargo.toml".into(), "Cargo.lock".into()]
        .into_iter()
        .chain(PKGS.map(|n| format!("crates/marmish-{n}/Cargo.toml")))
    {
        files[&p] = sha(&original.join(&p))?.into();
    }
    Ok(json!({"revision":ORIGINAL,"tree":tree.trim(),"files":files}))
}
fn git_pin(path: &Path, rev: &str) -> Result {
    let relative = path
        .strip_prefix(root())?
        .to_str()
        .ok_or("non-UTF-8 path")?;
    require(
        fs::read(path)?
            == run(Command::new("git")
                .current_dir(root())
                .args(["show", &format!("{rev}:{relative}")]))?,
        format!("frozen input changed: {relative}"),
    )
}
fn frozen() -> Result {
    let inventory = whole().join("public-types");
    git_pin(&inventory.join("provenance.json"), BASE_PIN)?;
    for name in PKGS {
        for suffix in ["exports.json", "exports.lock", "evidence.json"] {
            git_pin(
                &inventory.join(format!("original-{name}")).join(suffix),
                BASE_PIN,
            )?;
        }
    }
    let darwin = whole().join("canonical-workspace/aarch64-apple-darwin.json");
    git_pin(&darwin, "55da1b628138d48d1984f87f173a061886d48ea8")?;
    require(
        load(&darwin)?["workspaceLockSha256"]
            == "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4",
        "wrong canonical Darwin evidence",
    )
}
fn one(mut paths: impl Iterator<Item = PathBuf>) -> Result<PathBuf> {
    let value = paths.next().ok_or("missing native artifact")?;
    require(paths.next().is_none(), "ambiguous native artifact")?;
    Ok(value)
}
struct Artifacts {
    roles: Value,
    links: BTreeSet<String>,
    files: BTreeMap<String, String>,
    serde: PathBuf,
    serde_json: PathBuf,
}
fn artifacts(bytes: &[u8], product: &Path, target: &Path, host: &str) -> Result<Artifacts> {
    let rows: Vec<Value> = std::str::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.last() == Some(&json!({"reason":"build-finished","success":true})),
        "incomplete Cargo build",
    )?;
    let mut roles = json!({});
    let mut links = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut libs: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    for row in rows {
        if row["reason"] == "build-script-executed" {
            for p in list(&row["linked_paths"])? {
                links.insert(text(p)?.to_owned());
            }
            continue;
        }
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let paths = list(&row["filenames"])?
            .iter()
            .map(|v| Ok(PathBuf::from(text(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let name = text(&row["target"]["name"])?;
        let package = name.replace('_', "-");
        if row["target"]["kind"] == json!(["lib"])
            && PKGS.iter().any(|n| package == format!("marmish-{n}"))
        {
            let dir = product.join(&package);
            require(
                Path::new(text(&row["manifest_path"])?).canonicalize()? == dir.join("Cargo.toml")
                    && Path::new(text(&row["target"]["src_path"])?)
                        .canonicalize()?
                        .starts_with(&dir)
                    && row["profile"]["opt_level"] == "3"
                    && row["profile"]["test"] == false,
                "wrong generated native library provenance",
            )?;
            let artifact = |ext: &str| -> Result<PathBuf> {
                let p = one(paths
                    .iter()
                    .filter(|p| p.extension().is_some_and(|e| e == ext))
                    .cloned())?
                .canonicalize()?;
                require(p.starts_with(target), "library outside target directory")?;
                Ok(p)
            };
            require(
                roles.get(&package).is_none(),
                "duplicate generated native library",
            )?;
            roles[package] = json!([
                artifact("rmeta")?,
                artifact("rlib")?,
                row["features"],
                target.join("release/deps"),
                row["fresh"]
            ]);
        }
        for p in paths {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "rlib"
                && ["serde", "serde_json"].contains(&name)
                && row["target"]["kind"] == json!(["lib"])
                && row["profile"]["opt_level"] == "3"
                && p.starts_with(target.join(host).join("release"))
            {
                libs.entry(name.into()).or_default().insert(p.clone());
            }
            if ["rmeta", "rlib", if host == LINUX { "so" } else { "dylib" }].contains(&ext) {
                if host == LINUX {
                    require(p == p.canonicalize()?, "library closure contains symlink")?;
                }
                files.insert(
                    p.strip_prefix(target)?
                        .to_str()
                        .ok_or("non-UTF-8 path")?
                        .into(),
                    sha(&p)?,
                );
            }
        }
    }
    require(
        roles.as_object().is_some_and(|v| v.len() == PKGS.len()) && !files.is_empty(),
        "incomplete native member closure",
    )?;
    Ok(Artifacts {
        roles,
        links,
        files,
        serde: one(libs.remove("serde").unwrap_or_default().into_iter())?,
        serde_json: one(libs.remove("serde_json").unwrap_or_default().into_iter())?,
    })
}
#[derive(Parser)]
struct Args {
    input_revision: String,
    framework: PathBuf,
    original: PathBuf,
    compiler_target: PathBuf,
    work: PathBuf,
    receipt: PathBuf,
    #[arg(long, requires = "prebuilt_sha256")]
    prebuilt: Option<PathBuf>,
    #[arg(long, requires = "prebuilt")]
    prebuilt_sha256: Option<String>,
}
fn execute(a: Args) -> Result {
    let framework = a.framework.canonicalize()?;
    let original = a.original.canonicalize()?;
    let work = std::path::absolute(a.work)?;
    checkout(&framework, FRAMEWORK, true)?;
    checkout(root(), &a.input_revision, true)?;
    checkout(&original, ORIGINAL, true)?;
    let parent = work.parent().ok_or("missing work parent")?.canonicalize()?;
    let work = parent.join(work.file_name().ok_or("missing work name")?);
    require(
        ![root(), &framework, &original]
            .iter()
            .any(|p| work.starts_with(p)),
        "use a separate assembly area",
    )?;
    composition::compose(portable(), true)?;
    frozen()?;
    let source = source_pin(&original)?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing host")?;
    require(
        [DARWIN, LINUX].contains(&host)
            && version
                .lines()
                .any(|s| s == format!("commit-hash: {COMPILER}")),
        "expected native pinned Rust 1.97.1",
    )?;
    let scratch = tempfile::tempdir()?;
    let scratch = scratch.path().canonicalize()?;
    let (exe, compiler_inputs) = if let Some(path) = a.prebuilt {
        let path = path.canonicalize()?;
        require(
            a.prebuilt_sha256.as_ref() == Some(&sha(&path)?),
            "prebuilt compiler digest differs from its build receipt",
        )?;
        (path.clone(), path)
    } else {
        fs::create_dir_all(&a.compiler_target)?;
        let cache = a.compiler_target.canonicalize()?;
        let target = cache.join(FRAMEWORK);
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
                .arg(&target),
        ))?;
        (target.join("debug/oddlang"), cache)
    };
    fs::create_dir(&work)?;
    let compiler = work.join("oddlang");
    fs::copy(exe, &compiler)?;
    if let Some(sha256) = a.prebuilt_sha256 {
        require(
            sha(&compiler)? == sha256,
            "staged compiler changed during copy",
        )?;
    }
    let inputs = work.join("inputs");
    let spec = portable().join(format!("{RELEASE}.json"));
    let manifest = stage(&spec, &inputs)?;
    stage(&portable().join(format!("{BASE}.json")), &inputs)?;
    let staged = inventory(&inputs)?;
    let cache = work.join("providers");
    let git = providers(
        &portable().join(text(&manifest["cargoLock"]["path"])?),
        &cache,
    )?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[root(), &original, &framework, &compiler_inputs, &scratch],
        &cache,
    )?;
    let denied = [
        original.join("crates/marmish-engine/src/model.rs"),
        original.join("crates/marmish-native/src/mdk_adapter.rs"),
        framework.join("src/lib.rs"),
        whole().join("engine-fieldless-choices/probe.rs"),
        whole().join("runtime-status-data/probe.rs"),
        whole().join("gap-reason-recovery/probe.rs"),
    ];
    sandbox.deny(&denied)?;
    let baseline = work.join("canonical");
    let product = work.join("candidate");
    for (release, output) in [(BASE, &baseline), (RELEASE, &product)] {
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-workspace", "rust"])
            .arg(inputs.join(format!("{release}.json")))
            .arg(output))?;
    }
    let workspace = product::snapshots(&baseline, &product, &a.input_revision)?;
    let target = work.join("target");
    let raw = run(sandbox
        .command(&product)?
        .args([
            "cargo",
            "build",
            "--workspace",
            "--lib",
            "--release",
            "--target",
            host,
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&target)
        .arg("--message-format=json"))?;
    fs::write(work.join("cargo-artifacts.jsonl"), &raw)?;
    let selected = artifacts(&raw, &product, &target, host)?;
    let imports = imports::observe(host, &original, &scratch, &selected.roles)?;
    let domains = domains::observe(
        &json!({"roles":selected.roles,"host":host,"work":scratch,"target":target,"links":selected.links,"serde":selected.serde,"serdeJson":selected.serde_json}),
    )?;
    require(
        workspace == product::snapshots(&baseline, &product, &a.input_revision)?,
        "sealed workspace changed while probing",
    )?;
    for (p, h) in &selected.files {
        require(
            sha(&target.join(p))? == *h,
            "Cargo library closure changed while probing",
        )?;
    }
    require(
        source == source_pin(&original)?,
        "pinned original inputs changed",
    )?;
    // Match Python's sorted, space-separated map encoding for the native Cargo closure seal.
    let rows = selected
        .files
        .iter()
        .map(|(path, digest)| {
            Ok(format!(
                "{}: {}",
                serde_json::to_string(path)?,
                serde_json::to_string(digest)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let report = json!({
        "format": "marmish-canonical-public-data-v1",
        "host": host,
        "rustc": version.lines().collect::<Vec<_>>(),
        "framework": FRAMEWORK,
        "companion": a.input_revision,
        "original": source,
        "compilerSha256": sha(&compiler)?,
        "candidateManifestSha256": sha(&spec)?,
        "artifactLogSha256": bytes_sha(&raw),
        "cargoLibraryClosureEntries": selected.files.len(),
        "cargoLibraryClosureSha256": bytes_sha(format!("{{{}}}",rows.join(", ")).as_bytes()),
        "sourceDenials": denied.len(),
        "gitProviders": git,
        "stagedInputs": staged,
        "workspace": workspace,
        "imports": imports,
        "publicData": domains,
    });
    fs::write(a.receipt, json_bytes(&report)?)?;
    Ok(())
}
fn main() -> Result {
    execute(Args::parse())
}
