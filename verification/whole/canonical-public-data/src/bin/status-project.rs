//! Recover and build the local status client against its pinned workspace inputs.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, root},
    cli_projection, list, load, require, text,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
const WORKSPACE: &str = "native-status-workspace.json";

fn stage(portable: &Path, destination: &Path) -> Result<()> {
    let portable = portable.canonicalize()?;
    let output = destination.join("oddlang/portable");
    let copy_pin = |base: &Path, descriptor: &Value| -> Result<PathBuf> {
        let path = base.join(text(&descriptor["path"])?).canonicalize()?;
        require(path.starts_with(base), "input pin escapes its base")?;
        let bytes = fs::read(&path)?;
        require(
            descriptor["sha256"] == bytes_sha(&bytes),
            "invalid input pin",
        )?;
        let dest = output.join(path.strip_prefix(&portable)?);
        fs::create_dir_all(dest.parent().ok_or("missing parent")?)?;
        fs::write(&dest, bytes)?;
        Ok(dest)
    };
    fs::create_dir_all(output.join("native-status-v1"))?;
    for name in [
        WORKSPACE,
        "native-status-v1/workspace.lock",
        "native-status-v1/package.lock",
    ] {
        fs::copy(portable.join(name), output.join(name))?;
    }
    let spec = load(&output.join(WORKSPACE))?;
    copy_pin(&portable, &spec["cargoLock"])?;
    for member in spec["members"]
        .as_object()
        .ok_or("expected workspace members")?
        .values()
    {
        let staged = copy_pin(&portable, &member["manifest"])?;
        let package = load(&staged)?;
        let base = portable.join(
            staged
                .strip_prefix(&output)?
                .parent()
                .ok_or("missing package parent")?,
        );
        copy_pin(&base, &package["cargoLock"])?;
        for item in list(&package["components"])? {
            for name in ["graph", "overlay"] {
                copy_pin(&base, &item[name])?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let portable = args.root.join("oddlang/portable");
    let status = portable.join("native-status-v1");
    match args.action {
        Action::Stage { directory } => stage(&portable, &directory),
        Action::Generate { oddlang, directory } => cli_projection::generate(
            &oddlang,
            &portable.join(WORKSPACE),
            &directory,
            &status,
            &[
                ("Cargo.lock", "client-Cargo.lock"),
                ("workspace.lock", "workspace.lock"),
                ("marmish-native/package.lock", "package.lock"),
            ],
        ),
        Action::Recover { oddlang, directory } => {
            cli_projection::recover(
                &oddlang.canonicalize()?,
                &directory,
                &portable.join("native-cli-v1"),
            )?;
            for (name, digest) in [
                (
                    "crates/marmish-native/src/ipc.rs",
                    "ffeeba6b2d0b8fef1d44bea311e5548e8b7ac520d4b7d04286077547273b8a4a",
                ),
                (
                    "crates/marmish-native/src/error.rs",
                    "54eb8a335f91cb09de6ecd8d598acdec3476ee7067968333f85c824a5fee2254",
                ),
                (
                    "crates/marmish-native/src/transport.rs",
                    "5490e5d39761df1fc386249c3e901a920c4343257836908a6f46746a22c06800",
                ),
            ] {
                cli_projection::pinned(&directory, name, digest)?;
            }
            let lock = cli_projection::pinned(
                &directory,
                "Cargo.lock",
                "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
            )?;
            require(
                cli_projection::registry(&fs::read(status.join("client-Cargo.lock"))?)?
                    .is_subset(&cli_projection::registry(&lock)?),
                "status dependencies differ from the reference",
            )
        }
    }
}
