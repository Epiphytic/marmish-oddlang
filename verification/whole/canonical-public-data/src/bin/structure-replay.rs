//! Collect the original compiler observations for each archived structural role.
use clap::Parser;
use public_data_qualification::{Result, assembly, load, require, run, sha, whole};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use std::{fs, path::PathBuf, process::Command};

const FRAMEWORK: &str = "b653f1ada68791cd2fb70293bf227c5665850d1e";
const SOURCE: &str = "af54339d250290490d62d4475a77539b7e739071";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    upstream: PathBuf,
    output: PathBuf,
    #[arg(long)]
    triple: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let archives = whole().join("structure");
    require(
        !args.triple.is_empty()
            && args
                .triple
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid target triple",
    )?;
    let expected = archives.join(&args.triple);
    require(
        expected.is_dir(),
        format!("missing archived structural baseline for {}", args.triple),
    )?;
    let output = std::path::absolute(args.output)?;
    require(!output.try_exists()?, "output already exists")?;
    let framework = args.framework.canonicalize()?;
    let upstream = args.upstream.canonicalize()?;
    assembly::checkout(&framework, FRAMEWORK, false)?;
    assembly::checkout(&upstream, SOURCE, false)?;
    let temp = tempfile::tempdir()?;
    let mut build = Command::new("cargo");
    for key in [
        "RUSTC",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        build.env_remove(key);
    }
    run(build
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
        .arg(temp.path()))?;
    let base: toml::Table =
        fs::read_to_string(framework.join("examples/marmish/portable-contract/structure.toml"))?
            .parse()?;
    let adapter = framework.join("verification/rust-recovery/run.sh");
    let parent = output.parent().ok_or("missing output parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    for (package, kind, target) in [
        ("marmish-stream", "lib", "marmish_stream"),
        ("marmish-prototype", "lib", "marmish_prototype"),
        ("marmish-prototype", "bin", "marmish-engine-bridge"),
    ] {
        let mut profile = base.clone();
        for (key, value) in [
            ("sourceRevision", SOURCE),
            ("adapter", adapter.to_str().ok_or("invalid adapter path")?),
            ("package", package),
            ("targetKind", kind),
            ("targetName", target),
            ("targetTriple", args.triple.as_str()),
        ] {
            profile.insert(key.into(), toml::Value::String(value.into()));
        }
        let path = temp.path().join("profile.toml");
        fs::write(&path, toml::to_string(&profile)?)?;
        let actual = stage.path().join(target);
        run(Command::new(temp.path().join("debug/oddlang"))
            .args(["recover", "rust"])
            .arg(upstream.join("Cargo.toml"))
            .arg("--profile")
            .arg(path)
            .arg("--out")
            .arg(&actual))?;
        let lock = load(&actual.join("structure.lock"))?;
        require(
            lock == load(&expected.join(format!("{target}.lock.json")))?,
            format!("{target} differs from archived structural lock"),
        )?;
        require(
            lock["structureSha256"] == sha(&actual.join("structure.ttl"))?,
            format!("{target} graph digest mismatch"),
        )?;
    }
    renameat_with(CWD, stage.path(), CWD, &output, RenameFlags::NOREPLACE)?;
    Ok(())
}
