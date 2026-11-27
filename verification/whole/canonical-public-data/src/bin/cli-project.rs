//! Recover the original native parser graph or build its two parser-only executable roles.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result,
    assembly::root,
    cli_projection::{generate, recover},
};
use std::{fs, path::PathBuf};

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
        Action::Generate { oddlang, directory } => generate(
            &oddlang,
            &inputs.join("workspace.json"),
            &directory,
            &inputs,
            &[
                ("workspace.lock", "workspace.lock"),
                ("marmish-native/package.lock", "package.lock"),
            ],
        ),
    }
}
