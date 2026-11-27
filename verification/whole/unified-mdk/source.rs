//! Independently recover the build report, journal and MDK data from the original tree.
use public_data_qualification::{assembly::*, *};
use serde_json::{Value, json};
use std::path::Path;

#[path = "../release-info-structure/ingest.rs"]
mod release_ingest;

pub fn report(
    cli: &Path,
    original: &Path,
    inputs: &Path,
    workspace: &Value,
    scratch: &Path,
) -> Result<Value> {
    let member = load(&inputs.join(text(
        &workspace["members"]["marmish-native"]["manifest"]["path"],
    )?))?;
    let reports: Vec<_> = list(&member["components"])?
        .iter()
        .filter(|r| r["kind"] == "build-report")
        .collect();
    let [component] = reports.as_slice() else {
        return Err("expected one build-report component".into());
    };
    let receipt = whole().join("release-info-structure/darwin-arm64/receipt.json");
    let prior = load(&receipt)?;
    let lane = &prior["lanes"]["baseline"];
    let graph = sha(&inputs.join(text(&component["graph"]["path"])?))?;
    require(
        prior["representedRevision"] == ORIGINAL
            && lane["sourceRevision"] == ORIGINAL
            && lane["composedGraphSha256"] == graph
            && component["graph"]["sha256"] == graph,
        "recovered report differs from standalone qualification",
    )?;
    let extracted = scratch.join("original-report");
    release_ingest::recover(
        cli,
        original,
        &extracted,
        &scratch.join("report-selection.json"),
    )?;
    let mut candidate = load(&extracted.join("graph.json"))?;
    candidate["guards"] =
        load(&portable().join("native-build-report-v1/graph.json"))?["guards"].clone();
    require(
        candidate == load(&inputs.join("native-build-report-recovered-v1/graph.json"))?,
        "source-recovered report graph changed",
    )?;
    Ok(json!({"graphSha256":graph,"recoveryReceiptSha256":sha(&receipt)?}))
}
#[path = "../journal-records/recovery.rs"]
mod journal_recovery;
pub use journal_recovery::journal;
