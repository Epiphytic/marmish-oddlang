//! Select the sealed terminal gate's immediate help and version observations.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result,
    artifacts::cargo_rows,
    assembly::{DARWIN, LINUX},
    list, require, text,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{self, Read},
    path::{Path, PathBuf},
};

const ROLES: [&str; 2] = ["marmish", "marmishd"];

const ROOT_FORMS: [&str; 5] = ["--help", "-h", "--version", "-V", "help"];
const HELP_TOPICS: &str =
    "copy-receipts put get rsh stream streams stream-reconcile keychain-authorize release-info
install install-status uninstall setup init commands command-status cancel wait output daemon
status open-conversation send received foundation-check command-check help command-evidence
command-cleanup-status command-local-cancel command-prepare-cleanup";

fn arguments(row: &Value) -> Result<Vec<&str>> {
    list(&row["args"])?.iter().map(text).collect()
}

fn classify(archive: &Value) -> Result<Value> {
    let expected: BTreeSet<_> = ROOT_FORMS
        .into_iter()
        .map(|name| vec![name])
        .chain(
            HELP_TOPICS
                .split_whitespace()
                .map(|topic| vec!["help", topic]),
        )
        .collect();
    let mut result = json!({});
    for role in ROLES {
        let record = &archive["roles"][role];
        let mut remaining = expected.clone();
        let mut selected = Vec::new();
        let mut excluded = Vec::new();
        for row in list(&record["cases"])? {
            let args = arguments(row)?;
            if expected.contains(&args) {
                require(remaining.remove(&args), "duplicate terminal case")?;
                require(
                    row["exit"] == 0 && row["stderr"] == "" && !text(&row["stdout"])?.is_empty(),
                    "invalid terminal observation",
                )?;
                selected.push(row);
            } else {
                excluded.push(args);
            }
        }
        let outside = arguments(&record["outsideTerminalProfile"])?;
        require(
            remaining.is_empty() && excluded.len() == 46 && !expected.contains(&outside),
            "terminal corpus membership changed",
        )?;
        result[role] = json!([selected, excluded, outside]);
    }
    Ok(result)
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Read a successful Cargo JSON event stream and identify both release executables.
    LinkedBinaries {
        product: PathBuf,
        target: PathBuf,
        host: String,
    },
}

fn linked_binaries(product: &Path, target: &Path, host: &str, log: &[u8]) -> Result<Value> {
    require(matches!(host, DARWIN | LINUX), "unsupported terminal host")?;
    let manifest = product.canonicalize()?.join("marmish-native/Cargo.toml");
    let release = target.canonicalize()?.join(host).join("release");
    let mut binaries = json!({});
    for row in cargo_rows(log)? {
        if text(&row["reason"])? != "compiler-artifact" || row["target"]["kind"] != json!(["bin"]) {
            continue;
        }
        let role = text(&row["target"]["name"])?;
        require(
            ROLES.contains(&role) && binaries.get(role).is_none(),
            "unexpected or duplicate terminal executable",
        )?;
        let path = Path::new(text(&row["executable"])?).canonicalize()?;
        require(
            row["profile"]["test"] == false
                && row["profile"]["opt_level"] == "3"
                && Path::new(text(&row["manifest_path"])?).canonicalize()? == manifest
                && path == release.join(role)
                && path.is_file(),
            "wrong terminal executable provenance",
        )?;
        binaries[role] = json!(path);
    }
    require(
        ROLES.iter().all(|role| binaries.get(role).is_some()),
        "missing terminal executable",
    )?;
    Ok(binaries)
}

fn main() -> Result {
    let report = match Args::parse().action {
        None => classify(&serde_json::from_reader(io::stdin().lock())?)?,
        Some(Action::LinkedBinaries {
            product,
            target,
            host,
        }) => {
            let mut log = Vec::new();
            io::stdin().lock().read_to_end(&mut log)?;
            linked_binaries(&product, &target, &host, &log)?
        }
    };
    serde_json::to_writer(io::stdout().lock(), &report)?;
    Ok(())
}
