//! Independently recover the build report, journal and MDK data from the original tree.
use public_data_qualification::*;
use serde_json::Value;
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
    let baseline = recovered_vault::report(inputs, workspace)?;
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
    Ok(baseline)
}
#[path = "../journal-records/recovery.rs"]
mod journal_recovery;
pub use journal_recovery::journal;
