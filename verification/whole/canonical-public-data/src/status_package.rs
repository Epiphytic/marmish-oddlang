//! Recover and project the typed status and endpoint-discovery packages.
use crate::{Result, assembly::root, cli_projection as projection, list, load, require, run, text};
use clap::{Parser, Subcommand};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Typed,
    Discovery,
}

impl Profile {
    fn paths(self) -> (&'static str, &'static str) {
        match self {
            Self::Typed => ("native-status-typed-package.json", "native-status-typed-v1"),
            Self::Discovery => (
                "native-status-discovery-package.json",
                "native-status-discovery-v1",
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
fn stage(profile: Profile, portable: &Path, out: &Path) -> Result<()> {
    let (manifest, dir) = profile.paths();
    let staged = out.join("oddlang/portable");
    fs::create_dir_all(staged.join(dir))?;
    fs::copy(portable.join(manifest), staged.join(manifest))?;
    fs::copy(
        portable.join(dir).join("package.lock"),
        staged.join(dir).join("package.lock"),
    )?;
    if profile == Profile::Discovery {
        fs::copy(
            portable.join(dir).join("endpoint-obligations.json"),
            staged.join(dir).join("endpoint-obligations.json"),
        )?;
    }
    let package = load(&staged.join(manifest))?;
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

fn verify_sources(original: &Path, shape: &Path, profile: Profile) -> Result<()> {
    let provenance = load(&shape.join("provenance.json"))?;
    require(
        provenance["originalRevision"] == projection::REVISION,
        match profile {
            Profile::Typed => "typed reply reference revision differs",
            Profile::Discovery => "endpoint reference revision differs",
        },
    )?;
    for (name, hash) in provenance["sourceSha256"]
        .as_object()
        .ok_or("expected source pins")?
    {
        projection::pinned(original, name, text(hash)?)?;
    }
    Ok(())
}

fn verify_endpoint(compiler: &Path, portable: &Path, shape: &Path, output: &Path) -> Result<()> {
    let stage = tempfile::tempdir_in(
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let graph = stage.path().join("projection");
    run(Command::new(compiler)
        .args(["build", "local-command-client", "rust"])
        .arg(shape.join("graph.json"))
        .arg(portable.join("native-status-v1/rust.json"))
        .arg(&graph))?;
    require(
        fs::read(graph.join("endpoint-obligations.json"))?
            == fs::read(shape.join("endpoint-obligations.json"))?,
        "endpoint obligations differ",
    )
}

pub fn execute(profile: Profile) -> Result<()> {
    let (manifest, dir) = profile.paths();
    let args = Args::parse();
    let portable = args.root.join("oddlang/portable").canonicalize()?;
    let shape = portable.join(dir);
    match args.action {
        Action::Stage { directory } => stage(profile, &portable, &directory),
        Action::Recover { oddlang, directory } => {
            projection::recover_status(&oddlang.canonicalize()?, &directory, &portable)?;
            let (typed_manifest, typed_shape) = Profile::Typed.paths();
            verify_sources(&directory, &portable.join(typed_shape), Profile::Typed)?;
            if profile == Profile::Discovery {
                verify_sources(&directory, &shape, Profile::Discovery)?;
            }
            let lock = load(&portable.join(typed_manifest))?;
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
            let compiler = oddlang.canonicalize()?;
            if profile == Profile::Discovery {
                verify_endpoint(&compiler, &portable, &shape, &directory)?;
            }
            run(Command::new(compiler)
                .args(["build", "native-package", "rust"])
                .arg(portable.join(manifest))
                .arg(&directory))?;
            require(
                fs::read(directory.join("package.lock"))? == fs::read(shape.join("package.lock"))?,
                match profile {
                    Profile::Typed => "generated typed status package lock differs",
                    Profile::Discovery => "status discovery package lock differs",
                },
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
