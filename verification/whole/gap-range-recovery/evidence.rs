//! Check the rustdoc collection recipe, toolchain, sources and documents for both lanes.
use public_data_qualification::{
    Result,
    assembly::{COMPILER, DARWIN, LINUX, ORIGINAL, bytes_sha, root},
    load, require, run, sha,
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

pub(super) fn historical_digest(revision: &str, relative: &str) -> Result<String> {
    let bytes = run(Command::new("git")
        .current_dir(root())
        .args(["show", &format!("{revision}:{relative}")]))?;
    Ok(bytes_sha(&bytes))
}

fn runner_digest(evidence: &Value) -> Result<String> {
    match evidence["format"].as_str() {
        Some("marmish-linked-range-rustdoc-v2") => Ok(bytes_sha(include_bytes!("rustdoc.rs"))),
        Some("marmish-linked-range-rustdoc-v1") => {
            let helper = "verification/whole/gap-range-record/verify.py";
            let successor = historical_digest("393a561aca0103148bcc443c54c989309ac65bee", helper)?;
            let archived = historical_digest(super::LEGACY, helper)?;
            require(
                evidence["helperSha256"] == successor || evidence["helperSha256"] == archived,
                "unrecognized rustdoc helper",
            )?;
            historical_digest(
                "f3bbe1c50150ce2c2458e747f6c1efbd50dbf8b8",
                "verification/whole/gap-range-recovery/rustdoc.py",
            )
        }
        _ => Err("unrecognized rustdoc evidence version".into()),
    }
}

pub fn check(directory: &Path, original: &Path, mutation: &Path, host: &str) -> Result<Value> {
    let evidence = load(&directory.join("evidence.json"))?;
    let baseline = original.join(super::MODEL);
    require(
        [DARWIN, LINUX].contains(&host),
        "unsupported range evidence host",
    )?;
    let recipe = json!([
        "cargo",
        "+1.97.1",
        "rustdoc",
        "--locked",
        "--offline",
        "--lib",
        "-p",
        "marmish-engine",
        "--target",
        host,
        "--target-dir",
        "<target-dir>",
        "--",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
        "--document-private-items"
    ]);
    let environment =
        json!({"RUSTC_BOOTSTRAP":"1","CARGO_PROFILE_DEV_DEBUG":"0","CARGO_INCREMENTAL":"0"});
    require(
        evidence["original"] == ORIGINAL
            && evidence["host"] == host
            && evidence["source"] == super::MODEL
            && evidence["cargoLockSha256"] == sha(&original.join("Cargo.lock"))?
            && evidence["crateRootSha256"] == sha(&baseline.with_file_name("lib.rs"))?
            && evidence["cargoArgs"] == recipe
            && evidence["profileEnvironment"] == environment
            && evidence["runnerSha256"] == runner_digest(&evidence)?,
        "wrong rustdoc collection recipe or source",
    )?;
    let version = evidence["rustdoc"]
        .as_array()
        .ok_or("missing rustdoc toolchain")?;
    for required in [format!("commit-hash: {COMPILER}"), format!("host: {host}")] {
        require(
            version.iter().any(|line| line == &required),
            "wrong native rustdoc toolchain",
        )?;
    }
    for (label, source) in [("baseline", baseline.as_path()), ("mutation", mutation)] {
        let filename = format!("{label}.json");
        let bytes = fs::read(directory.join(&filename))?;
        let pinned = json!({"sourceSha256":sha(source)?, "rustdocSha256":bytes_sha(&bytes),"rustdocFile":filename});
        require(
            evidence["inputs"][label] == pinned,
            format!("wrong rustdoc inputs for {label}"),
        )?;
        let doc: Value = serde_json::from_slice(&bytes)?;
        let root_id = doc["root"]
            .as_u64()
            .ok_or("invalid rustdoc root")?
            .to_string();
        require(
            doc["format_version"] == 57
                && doc["includes_private"] == true
                && doc["target"]["triple"] == host
                && doc["index"][root_id]["name"] == "marmish_engine",
            format!("wrong compiled rustdoc document for {label}"),
        )?;
    }
    require(
        evidence["inputs"]["baseline"]["rustdocSha256"]
            != evidence["inputs"]["mutation"]["rustdocSha256"],
        "expected independent rustdoc builds",
    )?;
    Ok(evidence)
}
