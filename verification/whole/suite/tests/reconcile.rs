use serde_json::{Value, json};
use std::collections::BTreeMap;
use suite_receipts::{Result, read, reconcile, suite};

#[test]
fn named_receipts_match_both_platform_catalogs() -> Result<()> {
    for host in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let inventory = read(&suite().join(format!("named-release-{host}.json")))?;
        for candidate in [
            "canonical",
            "canonical-mdk",
            "canonical-public-data",
            "combined-v7",
            "manifest",
            "recovered",
            "seven",
        ] {
            let receipt = read(&suite().join(format!("{candidate}-{host}.json")))?;
            let mut observed = BTreeMap::new();
            for (name, row) in receipt["harnesses"].as_object().unwrap() {
                let mut cases: Vec<String> = serde_json::from_value(row["passed"].clone())?;
                cases.reverse();
                observed.insert(name.clone(), cases);
            }
            let (rows, totals) = reconcile(&inventory, observed)?;
            assert_eq!(rows, receipt["harnesses"], "{candidate} {host}");
            assert_eq!(totals, receipt["totals"], "{candidate} {host}");
        }
    }
    Ok(())
}

#[test]
fn empty_inventory_has_all_four_counts() -> Result<()> {
    let (rows, totals) = reconcile(&json!({"harnesses":{}}), BTreeMap::new())?;
    assert_eq!(rows, json!({}));
    assert_eq!(
        totals.to_string(),
        r#"{"passed":0,"unverified":0,"ignored":0,"benchmarks":0}"#
    );
    Ok(())
}

#[test]
fn reject_inconsistent_inventory_and_observations() {
    for (eligible, ignored, harness, passed) in [
        (json!(["a", "a"]), json!([]), "known", json!([])),
        (json!([]), json!(["a", "a"]), "known", json!([])),
        (json!(["a"]), json!(["a"]), "known", json!([])),
        (json!(["a"]), json!([]), "known", json!(["a", "a"])),
        (json!(["a"]), json!([]), "known", json!(["b"])),
        (json!(["a"]), json!([]), "missing", Value::Array(vec![])),
    ] {
        let inventory =
            json!({"harnesses":{"known":{"tests":eligible,"ignored":ignored,"benchmarks":[]}}});
        let observed =
            BTreeMap::from([(harness.to_owned(), serde_json::from_value(passed).unwrap())]);
        assert!(reconcile(&inventory, observed).is_err());
    }
}
