//! Prepare the portable inputs checked by the isolated journal-record projector.
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, json_bytes, load, pin, portable, require, sha, text};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "../../../journal-records/recovery.rs"]
mod recovery;
#[path = "../../../journal-records/witness.rs"]
mod witness;

const BUNDLE: &str = "native-journal-records-v1";
const PACKAGE: &str = "native-journal-records-package.json";
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Recover {
        compiler: PathBuf,
        original: PathBuf,
        scratch: PathBuf,
    },
    Probe {
        output: PathBuf,
    },
    Stage {
        destination: PathBuf,
        graphs: PathBuf,
    },
}

fn stage(dst: &Path, graphs: &Path) -> Result {
    fs::create_dir(dst)?;
    let bundle = dst.join(BUNDLE);
    fs::create_dir(&bundle)?;
    let mut manifest = load(&portable().join(PACKAGE))?;
    let lock = portable().join(BUNDLE).join("Cargo.lock");
    require(
        sha(&lock)? == text(&manifest["cargoLock"]["sha256"])?,
        "source Cargo.lock pin differs",
    )?;
    fs::copy(lock, bundle.join("Cargo.lock"))?;
    let [component] = manifest["components"]
        .as_array_mut()
        .ok_or("missing package components")?
        .as_mut_slice()
    else {
        return Err("expected one public-records component".into());
    };
    for (key, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
        fs::copy(graphs.join(file), bundle.join(file))?;
        component[key] = pin(dst, &format!("{BUNDLE}/{file}"))?;
    }
    fs::write(dst.join(PACKAGE), json_bytes(&manifest)?)?;
    Ok(())
}
fn main() -> Result {
    match Args::parse().command {
        Action::Recover {
            compiler,
            original,
            scratch,
        } => {
            println!("{}", recovery::journal(&compiler, &original, &scratch)?);
            Ok(())
        }
        Action::Probe { output } => {
            let input = std::io::read_to_string(std::io::stdin().lock())?;
            let rows = witness::cargo_rows(input.as_bytes())?;
            println!("{}", witness::journal_probe(&output, &rows)?);
            Ok(())
        }
        Action::Stage {
            destination,
            graphs,
        } => stage(&destination, &graphs),
    }
}
