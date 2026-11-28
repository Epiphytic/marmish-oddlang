//! Select the sealed terminal gate's immediate help and version observations.
use public_data_qualification::{Result, list, require, text};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io};

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
    for role in ["marmish", "marmishd"] {
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

fn main() -> Result {
    let archive = serde_json::from_reader(io::stdin().lock())?;
    serde_json::to_writer(io::stdout().lock(), &classify(&archive)?)?;
    Ok(())
}
