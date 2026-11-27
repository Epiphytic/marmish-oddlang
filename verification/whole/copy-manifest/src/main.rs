use clap::Parser;
use copy_manifest_replay::{cases, verify};
use public_data_qualification::Result;
use serde_json::Value;
use std::path::PathBuf;
#[cfg(test)]
use {public_data_qualification::assembly::bytes_sha, serde_json::json};
#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}
fn mutate(g: &mut Value) -> Result {
    let view = g["hashViews"]
        .as_object_mut()
        .and_then(|v| v.values_mut().next())
        .ok_or("no hash view")?;
    view["fields"]
        .as_array_mut()
        .ok_or("no namespace fields")?
        .swap(0, 1);
    let mut slots = g["layout"]
        .as_array_mut()
        .ok_or("no layout")?
        .iter_mut()
        .filter(|s| s["id"].as_str().is_some_and(|s| s.ends_with(":field:size")));
    let slot = slots.next().ok_or("size absent")?;
    assert!(slots.next().is_none());
    slot["encoding"]
        .as_object_mut()
        .ok_or("no encoding")?
        .remove("readError")
        .ok_or("no readError")?;
    Ok(())
}
fn main() -> Result {
    let a = Args::parse();
    verify(
        "copy-manifest",
        "e29873828a015a1af201e2887cae85ff4b2faeb8",
        &a.original,
        &a.framework,
        &a.receipt,
        cases(),
        mutate,
    )
}
#[test]
fn archived_observations() {
    let c = cases();
    let mut rows = json!(c.requests);
    rows.sort_all_objects();
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&rows).unwrap()),
        "13c337b7afd861651671b0bbecf6ece334fff585d66a7b1f1a85a289c53258b0"
    );
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&c.expected).unwrap()),
        "ef90454f108ea6d946a00008f73d37116d491710463ac806eee94792e3f57cc8"
    );
}
