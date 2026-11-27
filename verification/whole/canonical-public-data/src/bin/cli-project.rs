//! Recover the original native parser graph or build its two parser-only executable roles.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, root},
    require, run,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const REVISION: &str = "af54339d250290490d62d4475a77539b7e739071";
const INPUT_DIR: &str = "oddlang/portable/native-cli-v1";

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_os_t=root().to_path_buf())]
    root: PathBuf,
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    Recover {
        oddlang: PathBuf,
        directory: PathBuf,
    },
    Stage {
        directory: PathBuf,
    },
    Generate {
        oddlang: PathBuf,
        directory: PathBuf,
    },
}

fn registry(lock: &[u8]) -> Result<BTreeSet<[String; 4]>> {
    let doc: toml::Table = std::str::from_utf8(lock)?.parse()?;
    let mut pins = BTreeSet::new();
    for package in doc
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("expected lock packages")?
    {
        if package
            .get("source")
            .and_then(|s| s.as_str())
            .is_some_and(|s| s.starts_with("registry+"))
        {
            let field = |name: &str| -> Result<String> {
                Ok(package
                    .get(name)
                    .and_then(|s| s.as_str())
                    .ok_or("missing registry pin")?
                    .into())
            };
            pins.insert([
                field("name")?,
                field("version")?,
                field("source")?,
                field("checksum")?,
            ]);
        }
    }
    Ok(pins)
}

fn recover(compiler: &Path, original: &Path, inputs: &Path) -> Result<()> {
    let git = |args: &[&str]| run(Command::new("git").current_dir(original).args(args));
    require(
        std::str::from_utf8(&git(&["rev-parse", "HEAD"])?)?.trim_end() == REVISION,
        "unexpected original revision",
    )?;
    let pinned = |path: &str, digest: &str| -> Result<Vec<u8>> {
        let bytes = git(&["show", &format!("{REVISION}:{path}")])?;
        require(
            bytes_sha(&bytes) == digest,
            format!("source pin differs for {path}"),
        )?;
        Ok(bytes)
    };
    let manifest = pinned(
        "crates/marmish-native/Cargo.toml",
        "f92cdde82b3e4cf077294a7296117af4dcc28dba55bf1c9d694d7a4b273f539f",
    )?;
    let manifest: toml::Table = std::str::from_utf8(&manifest)?.parse()?;
    for (field, value) in [
        ("name", "marmish-native"),
        ("version", "0.3.1"),
        ("edition", "2024"),
    ] {
        require(
            manifest["package"][field].as_str() == Some(value),
            "unexpected original package metadata",
        )?;
    }
    let available = registry(&pinned(
        "Cargo.lock",
        "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    )?)?;
    let required = registry(&fs::read(inputs.join("parser-Cargo.lock"))?)?;
    require(
        !required.is_empty() && required.is_subset(&available),
        "parser dependency pins differ from the original release",
    )?;
    let tmp = tempfile::tempdir()?;
    let source = tmp.path().join("parser.rs");
    fs::write(
        &source,
        pinned(
            "crates/marmish-native/src/cli.rs",
            "a4122418f4f777a53e839487b3b162951ac2b1447ea26b968245322390c866d4",
        )?,
    )?;
    let graph = tmp.path().join("candidate.json");
    run(Command::new(compiler)
        .args(["recover", "cli-surface"])
        .arg(source)
        .arg(inputs.join("selection.json"))
        .arg(&graph))?;
    require(
        fs::read(graph)? == fs::read(inputs.join("graph.json"))?,
        "recovered graph differs from published candidate",
    )
}

fn main() -> Result<()> {
    let args = Args::parse();
    let inputs = args.root.join(INPUT_DIR);
    match args.action {
        Action::Recover { oddlang, directory } => {
            recover(&oddlang.canonicalize()?, &directory, &inputs)
        }
        Action::Stage { directory } => {
            let output = directory.join(INPUT_DIR);
            fs::create_dir_all(&output)?;
            for file in [
                "graph.json",
                "marmish.json",
                "marmishd.json",
                "package.json",
                "package.lock",
                "parser-Cargo.lock",
                "workspace.json",
                "workspace.lock",
            ] {
                fs::copy(inputs.join(file), output.join(file))?;
            }
            Ok(())
        }
        Action::Generate { oddlang, directory } => {
            run(Command::new(oddlang.canonicalize()?)
                .args(["build", "native-workspace", "rust"])
                .arg(inputs.join("workspace.json"))
                .arg(&directory))?;
            for (file, expected) in [
                ("workspace.lock", "workspace.lock"),
                ("marmish-native/package.lock", "package.lock"),
            ] {
                require(
                    fs::read(directory.join(file))? == fs::read(inputs.join(expected))?,
                    format!("generated {file} differs from the reviewed lock"),
                )?;
            }
            run(Command::new("cargo").current_dir(directory).args([
                "build",
                "--release",
                "--locked",
                "--offline",
                "--bins",
            ]))?;
            Ok(())
        }
    }
}
