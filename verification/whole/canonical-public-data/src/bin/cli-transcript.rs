//! Capture and verify the parse-only Marmish CLI transcript.
use clap::{Parser, ValueEnum};
use public_data_qualification::{Result, cli_transcript, json_bytes, load, require, sha, whole};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Capture,
    Verify,
}
#[derive(Parser)]
struct Args {
    mode: Mode,
    bindir: PathBuf,
    #[arg(long,default_value_os_t=whole().join("cli/transcript.json"))]
    fixture: PathBuf,
    #[arg(long,default_value_os_t=whole().join("bridge/aarch64-apple-darwin.json"))]
    build_receipt: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let binaries = args.bindir.canonicalize()?;
    if matches!(args.mode, Mode::Capture) {
        let receipt = load(&args.build_receipt)?;
        for role in cli_transcript::ROLES {
            require(
                receipt["builds"]["default"]["sha256"][role] == sha(&binaries.join(role))?,
                format!("unrecognized original binary: {role}"),
            )?;
        }
    }
    let observed = cli_transcript::collect(&binaries)?;
    match args.mode {
        Mode::Capture => {
            let mut stage = tempfile::NamedTempFile::new_in(
                args.fixture
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?;
            stage.write_all(&json_bytes(&observed)?)?;
            stage.persist_noclobber(args.fixture)?;
        }
        Mode::Verify => {
            let expected = load(&args.fixture)?;
            cli_transcript::verify(&observed, &expected)?;
        }
    }
    println!(
        "{{'marmish': {}, 'marmishd': {}}}",
        observed["marmish"].as_array().unwrap().len(),
        observed["marmishd"].as_array().unwrap().len()
    );
    Ok(())
}
