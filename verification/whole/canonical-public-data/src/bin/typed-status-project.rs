//! Recover and project the typed local status reply package.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, assembly::root, cli_projection as projection, list, load, require, run, text,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PACKAGE: &str = "native-status-typed-package.json";
const SHAPE: &str = "native-status-typed-v1";
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
fn stage(portable: &Path, out: &Path) -> Result<()> {
    let staged = out.join("oddlang/portable");
    fs::create_dir_all(staged.join(SHAPE))?;
    for name in [PACKAGE, "native-status-typed-v1/package.lock"] {
        fs::copy(portable.join(name), staged.join(name))?;
    }
    let package = load(&staged.join(PACKAGE))?;
    let copy = |pin| -> Result<()> {
        let (source, bytes) = projection::input_bytes(portable, pin)?;
        let dest = staged.join(source.strip_prefix(portable)?);
        fs::create_dir_all(dest.parent().ok_or("missing input directory")?)?;
        fs::write(dest, bytes)?;
        Ok(())
    };
    copy(&package["cargoLock"])?;
    for component in list(&package["components"])? {
        copy(&component["graph"])?;
        copy(&component["overlay"])?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let portable = args.root.join("oddlang/portable").canonicalize()?;
    let shape = portable.join(SHAPE);
    match args.action {
        Action::Stage { directory } => stage(&portable, &directory),
        Action::Recover { oddlang, directory } => {
            projection::recover_status(&oddlang.canonicalize()?, &directory, &portable)?;
            let provenance = load(&shape.join("provenance.json"))?;
            require(
                provenance["originalRevision"] == projection::REVISION,
                "typed reply reference revision differs",
            )?;
            for (name, hash) in provenance["sourceSha256"]
                .as_object()
                .ok_or("expected source pins")?
            {
                projection::pinned(&directory, name, text(hash)?)?;
            }
            let lock = load(&portable.join(PACKAGE))?;
            let (_, bytes) = projection::input_bytes(&portable, &lock["cargoLock"])?;
            let original = projection::pinned(
                &directory,
                "Cargo.lock",
                "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
            )?;
            require(
                projection::checksummed_packages(&bytes)?
                    .is_subset(&projection::checksummed_packages(&original)?),
                "typed status registry dependencies differ from original",
            )
        }
        Action::Generate { oddlang, directory } => {
            run(Command::new(oddlang.canonicalize()?)
                .args(["build", "native-package", "rust"])
                .arg(portable.join(PACKAGE))
                .arg(&directory))?;
            require(
                fs::read(directory.join("package.lock"))? == fs::read(shape.join("package.lock"))?,
                "generated typed status package lock differs",
            )?;
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
