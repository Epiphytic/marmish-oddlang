use clap::Parser;
use copy_manifest_replay::{helper_context::cases, verify};
use public_data_qualification::{Result, require};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

fn mutate(graph: &mut Value) -> Result {
    let slots = graph["layout"]
        .as_array_mut()
        .ok_or("missing record layout")?;
    require(slots.len() > 4, "missing identity slots")?;
    slots.swap(3, 4);
    Ok(())
}
fn main() -> Result {
    let a = Args::parse();
    verify(
        "copy-helper-context",
        "d427d5399a9ce91195eb1eb467dab99d294e45bb",
        &a.original,
        &a.framework,
        &a.receipt,
        cases(),
        mutate,
    )
}

#[test]
fn archived_requests_and_outputs() {
    use public_data_qualification::assembly::bytes_sha;
    let c = cases();
    let mut requests = Value::Array(c.requests);
    requests.sort_all_objects();
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&requests).unwrap()),
        "c7acdc2e0513ce674abcd6143a820be4cfbd03e13fde2abc310813b6588559c6"
    );
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&c.expected).unwrap()),
        "3c616ccfe696dda81c6a0317ce0c7b7373910290a6650fc44f4cda4138e5065f"
    );
}
