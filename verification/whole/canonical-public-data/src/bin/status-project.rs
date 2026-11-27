//! Recover and build the local status client against its pinned workspace inputs.
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, assembly::root, cli_projection, list, load};
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
        let (path, bytes) = cli_projection::input_bytes(base, descriptor)?;
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
            cli_projection::recover_status(&oddlang.canonicalize()?, &directory, &portable)
        }
    }
}
