//! Generate and compile the two parser-only binaries from a staged, pinned workspace.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{root, stage},
    capture, json_bytes, load,
    macos_build::publish,
    portable,
    product::check_members,
    require, run, sha, stdout,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const BASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered";
const SUCCESSOR: &str = "workspace-v7-offline-cli";

#[derive(Parser)]
struct Args {
    compiler: PathBuf,
    framework: PathBuf,
    original: PathBuf,
    output: PathBuf,
    target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    successor: bool,
}

fn members(lock: &Value) -> Result<&serde_json::Map<String, Value>> {
    Ok(lock["members"]
        .as_object()
        .ok_or("missing workspace members")?)
}

fn check_workspace(product: &Path, expected: &Path, lock: &Value) -> Result {
    require(
        fs::read(product.join("workspace.lock"))? == fs::read(expected)?,
        "generated workspace lock differs",
    )?;
    check_members(product, lock, true)?;
    for name in members(lock)?.keys() {
        let member = product.join(name);
        let mut names = BTreeSet::from(["Cargo.toml", "package.lock", "src"]);
        if name == "marmish-native" {
            names.insert("build.rs");
        }
        let actual = fs::read_dir(member)?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        require(
            actual == names.into_iter().map(Into::into).collect(),
            format!("unexpected generated package paths: {name}"),
        )?;
    }
    Ok(())
}

fn main() -> Result {
    let args = Args::parse();
    let out = std::path::absolute(&args.output)?;
    require(!out.try_exists()?, "expected an absent product directory")?;
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let revision = stdout(
        Command::new("git")
            .current_dir(&framework)
            .args(["rev-parse", "HEAD"]),
    )?;
    run(Command::new("git").current_dir(&framework).args([
        "diff",
        "--quiet",
        "HEAD",
        "--",
        "src",
        "Cargo.toml",
        "Cargo.lock",
    ]))?;
    let dir = tempfile::tempdir()?;
    let work = dir.path().canonicalize()?;
    let release = if args.successor { SUCCESSOR } else { BASE };
    let portable = portable();
    let filename = format!("{release}.json");
    let manifest = portable.join(&filename);
    let inputs = work.join("inputs");
    let spec = stage(&manifest, &inputs)?;
    let staged_compiler = work.join("oddlang");
    fs::copy(&args.compiler, &staged_compiler)?;
    let mut policy = String::from("(version 1) (allow default) (deny network*)");
    for source in [&framework, &original, &root().canonicalize()?] {
        policy.push_str(&format!(
            " (deny file-read* (subpath {}))",
            serde_json::to_string(source)?
        ));
    }
    let sandbox = || {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .args(["-p", &policy])
            .current_dir(&work)
            .env("RUSTUP_TOOLCHAIN", "1.97.1");
        command
    };
    for path in [
        framework.join("Cargo.toml"),
        original.join("Cargo.toml"),
        manifest.clone(),
    ] {
        require(
            path.is_file(),
            format!("source denial fixture missing: {}", path.display()),
        )?;
        let probe = capture(sandbox().arg("/bin/cat").arg(&path), 10, None)?;
        require(
            probe.status.code() == Some(1)
                && probe.stdout.is_empty()
                && String::from_utf8_lossy(&probe.stderr).contains("Operation not permitted"),
            format!("failed source isolation check: {}", path.display()),
        )?;
    }
    let product = work.join("product");
    run(sandbox()
        .arg(&staged_compiler)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(filename))
        .arg(&product))?;
    let reference = portable.join(release).join("workspace.lock");
    let lock = load(&reference)?;
    check_workspace(&product, &reference, &lock)?;
    let previous = load(&portable.join(BASE).join("workspace.lock"))?;
    let current = members(&lock)?;
    require(
        current.keys().collect::<BTreeSet<_>>() == members(&spec)?.keys().collect()
            && lock["cargoLockSha256"] == previous["cargoLockSha256"],
        "workspace member set or Cargo dependency closure changed",
    )?;
    for (name, row) in members(&previous)? {
        if name != "marmish-native" || !args.successor {
            require(
                current.get(name) == Some(row),
                format!("unrelated sealed workspace member changed: {name}"),
            )?;
        }
    }
    fs::create_dir_all(&args.target)?;
    let target = args.target.canonicalize()?;
    run(sandbox()
        .current_dir(&product)
        .args([
            "cargo",
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "--release",
            "-p",
            "marmish-native",
            "--bins",
            "--target-dir",
        ])
        .arg(&target))?;
    check_workspace(&product, &reference, &lock)?;
    let mut bins = json!({});
    for role in ["marmish", "marmishd"] {
        bins[role] = sha(&target.join("release").join(role))?.into();
    }
    let receipt = json!({"frameworkRevision":revision.trim(),"compilerSha256":sha(&staged_compiler)?,"workspaceLockSha256":sha(&reference)?,"workspaceManifestSha256":sha(&manifest)?,"binarySha256":bins});
    publish(&product, &out)?;
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    Ok(())
}
