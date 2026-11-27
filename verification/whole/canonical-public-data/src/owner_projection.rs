//! Project the status-only and foreground owner packages from pinned inputs.
use crate::{
    Result,
    assembly::root,
    cli_projection::{self, pinned, registry},
    list, load, require, run, stdout, text,
};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Status,
    Foreground,
}
impl Owner {
    fn paths(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Status => (
                "native-status-owner-v1/package.json",
                "native-status-owner-v1",
                "owner-Cargo.lock",
            ),
            Self::Foreground => (
                "native-foreground-package.json",
                "native-foreground-owner-v1",
                "foreground-Cargo.lock",
            ),
        }
    }
}
#[derive(Parser)]
struct Args {
    #[arg(long,default_value_os_t=root().to_path_buf())]
    root: PathBuf,
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Stage {
        directory: PathBuf,
    },
    Recover {
        oddlang: PathBuf,
        directory: PathBuf,
    },
    Generate {
        oddlang: PathBuf,
        directory: PathBuf,
    },
}

fn verify_owner(original: &Path, portable: &Path) -> Result<Vec<u8>> {
    require(
        stdout(
            Command::new("git")
                .arg("-C")
                .arg(original)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim_end()
            == cli_projection::REVISION,
        "unexpected original revision",
    )?;
    let manifest = pinned(
        original,
        "crates/marmish-native/Cargo.toml",
        "f92cdde82b3e4cf077294a7296117af4dcc28dba55bf1c9d694d7a4b273f539f",
    )?;
    let pkg: toml::Table = std::str::from_utf8(&manifest)?.parse()?;
    let owner = portable.join("native-status-owner-v1");
    let overlay = load(&owner.join("package.json"))?;
    for (field, value) in [
        ("name", "marmish-native"),
        ("version", "0.3.1"),
        ("edition", "2024"),
    ] {
        require(
            pkg["package"][field].as_str() == Some(value)
                && (field == "name" || overlay[field] == value),
            "owner release metadata differs",
        )?;
    }
    for (name, hash) in [
        (
            "daemon",
            "cf2e7cc2a9e9a5a55db726286a53e595d607cc22288cdf7b7fed0fe6c2f439c8",
        ),
        (
            "ipc",
            "ffeeba6b2d0b8fef1d44bea311e5548e8b7ac520d4b7d04286077547273b8a4a",
        ),
        (
            "identity",
            "0f8d1e4bcc9865ada76cc757b8bddcbc3a15397f0075c57d0eb6bd967b2a349d",
        ),
        (
            "command_lifecycle",
            "35b2114ae59fa078ce630f36c14ed7cf494d5dde2b3d594e673964bc34ca6f3c",
        ),
    ] {
        pinned(
            original,
            &format!("crates/marmish-native/src/{name}.rs"),
            hash,
        )?;
    }
    let lock = pinned(
        original,
        "Cargo.lock",
        "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    )?;
    verify_dependencies(&owner.join("owner-Cargo.lock"), &lock)?;
    Ok(lock)
}
fn verify_dependencies(file: &Path, original: &[u8]) -> Result<()> {
    let pins = registry(&fs::read(file)?)?;
    require(
        !pins.is_empty() && pins.is_subset(&registry(original)?),
        "owner dependencies differ from original",
    )
}
fn layout(graph: &Value, reader: &Value) -> Result<()> {
    for (left, right) in [
        ("/arguments/state", "/state_argument"),
        ("/arguments/identity", "/profile_argument"),
        ("/profile/document", "/profile/leaf"),
        ("/profile/maximum_bytes", "/profile/maximum_bytes"),
        ("/profile/name_limit", "/profile_atom/maximum_bytes"),
        ("/runtime/binding", "/binding/leaf"),
    ] {
        let expected = reader.pointer(right).ok_or("missing discovery field")?;
        require(
            graph.pointer(left) == Some(expected),
            "foreground and discovery fields differ",
        )?;
    }
    for (owner, reference) in [("profile", "profile"), ("runtime", "binding")] {
        let fields = graph[owner]["fields"]
            .as_object()
            .ok_or("missing field map")?
            .values()
            .map(text)
            .collect::<Result<BTreeSet<_>>>()?;
        let expected = list(&reader[reference]["fields"])?
            .iter()
            .map(|v| text(&v["key"]))
            .collect::<Result<BTreeSet<_>>>()?;
        require(
            fields == expected,
            "foreground field names differ from discovery",
        )?;
    }
    require(
        reader["runtime"][0]["value"] == format!("{}-", text(&graph["runtime"]["prefix"])?),
        "runtime prefix differs",
    )
}
pub fn execute(owner: Owner) -> Result<()> {
    let args = Args::parse();
    let portable = args.root.join("oddlang/portable").canonicalize()?;
    let (manifest, folder, cargo_lock) = owner.paths();
    let path = portable.join(manifest);
    let shape = portable.join(folder);
    match args.action {
        Action::Recover { oddlang, directory } => {
            let lock = verify_owner(&directory, &portable)?;
            if owner == Owner::Foreground {
                cli_projection::recover_status(&oddlang.canonicalize()?, &directory, &portable)?;
                layout(
                    &load(&shape.join("graph.json"))?,
                    &load(&portable.join("native-status-discovery-v1/graph.json"))?["discovery"],
                )?;
                verify_dependencies(&shape.join(cargo_lock), &lock)?;
            }
            Ok(())
        }
        Action::Stage { directory } => {
            let dest = directory.join("oddlang/portable");
            fs::create_dir_all(dest.join(folder))?;
            let package = load(&path)?;
            fs::copy(&path, dest.join(manifest))?;
            fs::copy(
                shape.join("package.lock"),
                dest.join(folder).join("package.lock"),
            )?;
            let copy = |pin: &Value| -> Result<()> {
                let (src, bytes) = cli_projection::input_bytes(
                    path.parent().ok_or("missing manifest parent")?,
                    pin,
                )?;
                let target = dest.join(src.strip_prefix(&portable)?);
                fs::create_dir_all(target.parent().ok_or("missing target directory")?)?;
                fs::write(target, bytes)?;
                Ok(())
            };
            copy(&package["cargoLock"])?;
            for component in list(&package["components"])? {
                copy(&component["graph"])?;
                copy(&component["overlay"])?;
            }
            Ok(())
        }
        Action::Generate { oddlang, directory } => {
            run(Command::new(oddlang.canonicalize()?)
                .args(["build", "native-package", "rust"])
                .arg(path)
                .arg(&directory))?;
            for (actual, expected) in [("Cargo.lock", cargo_lock), ("package.lock", "package.lock")]
            {
                require(
                    fs::read(directory.join(actual))? == fs::read(shape.join(expected))?,
                    format!("generated {actual} differs from the reviewed lock"),
                )?;
            }
            run(Command::new("cargo").current_dir(directory).args([
                "build",
                "--release",
                "--locked",
                "--offline",
            ]))?;
            Ok(())
        }
    }
}
