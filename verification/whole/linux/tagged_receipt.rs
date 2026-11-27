//! Compare the public guest observations against the reviewed tagged-data oracle.
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, bytes_sha},
    baseline_framework, load, require,
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    path::Path,
};

const LOCK: &str = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3";

pub fn verify(framework: &Path) -> Result<Value> {
    baseline_framework::verify(framework)?;
    let expected =
        load(&framework.join("examples/marmish/portable-oracle/tagged-qualification.json"))?;
    let mut raw = Vec::new();
    io::stdin().lock().read_to_end(&mut raw)?;
    let observed: Value = serde_json::from_slice(&raw)?;
    let pins = json!({
        "linux_boundary.py": "559e56472735f81ca0400e221c207b202a632b0358fd8d0e3695adeefe30e387",
        "qualify_tagged.py": "f2fd54fc8768215eb41d4052b8194f15699f4bc72a5266a9e07ebf7086b6a973",
        "tagged_adapter.mjs": "5be9dd353fabb41c58439c4b3dda71e1a1252e648c24b5a6d3d8eac737643c52",
        "tagged_adapter.rs": "8f8315f5975326950ec9ca9bcb08192e9b382f7951c642a0e4f3e000247cf2f1"
    });
    require(
        observed["driverSourceSha256"] == pins,
        "unexpected guest qualifier or adapter",
    )?;
    for field in [
        "approvedGraphSha256",
        "publicCaseIds",
        "comparedOperationResults",
        "mixedPeers",
        "mutation",
    ] {
        require(
            observed.get(field) == Some(expected.get(field).ok_or("missing baseline observation")?),
            format!("public receipt differs at {field}"),
        )?;
    }
    require(
        observed["upstreamRevision"] == ORIGINAL
            && observed["originalCaller"]["revision"] == ORIGINAL
            && observed["originalCaller"]["cargoLockSha256"] == LOCK,
        "unexpected original revision or lock",
    )?;
    let sources = expected
        .pointer("/build/generatedSourceSha256")
        .ok_or("missing generated source pins")?;
    require(
        observed.pointer("/build/generatedSourceSha256") == Some(sources),
        "public receipt has unexpected generated source",
    )?;
    Ok(json!({"guestReceiptSha256": bytes_sha(&raw), "generatedSourceSha256":sources}))
}
