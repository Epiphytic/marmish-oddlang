use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::process::Command;
use suite_receipts::{candidate_targets, inspect, read, suite};

// The fixtures name the individual changes exercised by the predecessor's negative tests.
fn mutate(doc: &mut Value, op: &Value) {
    if let Some(p) = op["append"].as_str() {
        doc.pointer_mut(p)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(op["value"].clone());
        return;
    }
    let path = op["set"].as_str().or(op["remove"].as_str()).unwrap();
    let (parent, key) = path.rsplit_once('/').unwrap();
    let key = key.replace("~1", "/").replace("~0", "~");
    let node = doc.pointer_mut(parent).unwrap();
    if op.get("set").is_some() {
        if let Some(a) = node.as_array_mut() {
            a[key.parse::<usize>().unwrap()] = op["value"].clone();
        } else {
            node[key] = op["value"].clone();
        }
    } else if let Some(a) = node.as_array_mut() {
        a.remove(key.parse::<usize>().unwrap());
    } else {
        node.as_object_mut().unwrap().remove(&key).unwrap();
    }
}

#[test]
fn receipt_mutations_are_rejected() {
    let cases = read(&suite().join("tests/refusal-cases.json")).unwrap();
    let mut exercised = 0;
    for case in cases.as_array().unwrap() {
        for archive in case["receipts"].as_array().unwrap() {
            let mut doc =
                read(&suite().join(format!("{}.json", archive.as_str().unwrap()))).unwrap();
            inspect(&doc).unwrap();
            for op in case["operations"].as_array().unwrap() {
                mutate(&mut doc, op);
            }
            assert!(inspect(&doc).is_err(), "{archive}: {}", case["change"]);
            exercised += 1;
        }
    }
    assert_eq!(exercised, 66);
}

#[test]
fn archive_totals_targets_and_incomplete_exit() {
    let report_hashes = read(&suite().join("tests/target-report-sha256.json")).unwrap();
    for (family, counts) in [
        ("combined-v7", [37, 37]),
        ("recovered", [39, 40]),
        ("seven", [40, 41]),
        ("manifest", [42, 43]),
        ("canonical", [42, 43]),
        ("canonical-mdk", [43, 44]),
        ("canonical-public-data", [42, 43]),
    ] {
        for (host, passed) in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"]
            .into_iter()
            .zip(counts)
        {
            let name = format!("{family}-{host}.json");
            let file = suite().join(&name);
            let doc = read(&file).unwrap();
            let out = Command::new(env!("CARGO_BIN_EXE_check-coverage"))
                .arg(&file)
                .arg("--require-complete")
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(1));
            assert!(out.stderr.is_empty());
            let totals: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(totals, doc["totals"]);
            assert_eq!(totals["passed"], passed);
            if doc.get("builtLibraries").is_some() {
                let out = Command::new(env!("CARGO_BIN_EXE_candidate-targets"))
                    .arg(&file)
                    .output()
                    .unwrap();
                assert!(out.status.success());
                assert_eq!(
                    report_hashes[&name],
                    format!("{:x}", Sha256::digest(&out.stdout))
                );
                let report: Value = serde_json::from_slice(&out.stdout).unwrap();
                let targets = report["targets"].as_array().unwrap();
                assert_eq!(report["extraObserved"], json!([]));
                assert!(targets.iter().any(|row| row["buildObserved"] == true));
                assert!(targets.iter().any(|row| row["buildObserved"] == false));
            } else {
                assert!(candidate_targets(&doc).is_err());
            }
        }
    }
}
