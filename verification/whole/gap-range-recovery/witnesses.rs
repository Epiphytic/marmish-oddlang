//! Load the independent native observations and their linked choice evidence.
use public_data_qualification::{
    Result,
    assembly::{COMPILER, DARWIN, LINUX, ORIGINAL},
    list, load, portable,
    product::pinned,
    require, sha, text, whole,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn ids(rows: &[Value]) -> Result<Vec<&str>> {
    rows.iter().map(|row| text(&row["id"])).collect()
}

pub(super) fn check_observations(
    cases: &[Value],
    baseline: &[Value],
    mutation: &[Value],
) -> Result {
    let names = ids(cases)?;
    require(
        names.len() == 35
            && names.iter().collect::<BTreeSet<_>>().len() == 35
            && ids(baseline)? == names
            && ids(mutation)? == names,
        "wrong native case inventory",
    )?;
    let changed: Vec<_> = baseline
        .iter()
        .zip(mutation)
        .filter(|(a, b)| a != b)
        .collect();
    let [(before, after)] = changed.as_slice() else {
        return Err("expected one changed native case".into());
    };
    require(
        before["id"] == "unknown-field"
            && before["result"] == json!({"encoded":r#"{"start":0,"end":null,"reason":"Limit"}"#})
            && after["result"]["diagnostic"]
                .as_str()
                .is_some_and(|s| s.starts_with("unknown field `ignored`")),
        "unexpected change in native unknown-field behavior",
    )
}

pub(super) fn original_reference(original: &Path, host: &str) -> Result<Value> {
    let fixture = whole().join("gap-range-reference");
    let reference = load(&fixture.join(format!("original-{host}.json")))?;
    let source = sha(&original.join(super::MODEL))?;
    let cases_sha = sha(&fixture.join("cases.json"))?;
    let probe_sha = sha(&fixture.join("probe.rs"))?;
    let observer = super::evidence::historical_digest(
        "2c69f8c38cbd57136eb684f5b515ef1490f017b3",
        "verification/whole/gap-range-reference/observe.py",
    )?;
    require(
        reference["original"] == ORIGINAL
            && reference["host"] == host
            && reference["modelSha256"] == source
            && reference["cargoLockSha256"] == sha(&original.join("Cargo.lock"))?
            && reference["casesSha256"] == cases_sha
            && reference["probeSha256"] == probe_sha
            && reference["observerSha256"] == observer,
        "original reference provenance differs",
    )?;
    Ok(reference)
}

pub fn check(original: &Path, mutation: &Path, host: &str, rustc: &Value) -> Result<Value> {
    require(
        [DARWIN, LINUX].contains(&host),
        "unsupported native witness host",
    )?;
    let lines = list(rustc)?;
    require(
        lines.contains(&json!(format!("host: {host}")))
            && lines.contains(&json!(format!("commit-hash: {COMPILER}"))),
        "wrong native compiler",
    )?;
    let source = sha(&original.join(super::MODEL))?;
    let fixture = whole().join("gap-range-reference");
    let native_path = whole()
        .join("gap-range-record")
        .join(format!("{host}.json"));
    pinned(&native_path, super::LEGACY)?;
    let native = load(&native_path)?;
    let reference_path = fixture.join(format!("original-{host}.json"));
    let reference = original_reference(original, host)?;
    let cases = fixture.join("cases.json");
    let cases_sha = sha(&cases)?;
    let probe_sha = sha(&fixture.join("probe.rs"))?;
    let reference_sha = sha(&reference_path)?;
    require(
        native["format"] == "marmish-public-gap-range-v1"
            && native["original"] == ORIGINAL
            && native["rustc"] == *rustc
            && native["sourceModelSha256"] == source
            && native["mutationSourceModelSha256"] == sha(mutation)?
            && native["changedRows"] == json!(["unknown-field"])
            && native["originalReferenceSha256"] == reference_sha
            && native["baseline"]["observations"] == reference["results"]
            && native["casesSha256"] == cases_sha
            && native["unchangedProbeSha256"] == probe_sha
            && native["nativeTraitsProbeSha256"] == sha(&native_path.with_file_name("traits.rs"))?,
        "archived native qualification differs",
    )?;
    let baseline = &reference["results"];
    let changed = &native["mutation"]["observations"];
    check_observations(list(&load(&cases)?)?, list(baseline)?, list(changed)?)?;

    let reason_path = whole()
        .join("gap-reason-recovery")
        .join(format!("{host}.json"));
    pinned(&reason_path, super::REASON)?;
    let reason = load(&reason_path)?;
    require(
        reason["original"] == ORIGINAL
            && reason["host"] == host
            && reason["sourceSha256"] == source,
        "linked choice source provenance differs",
    )?;
    for (key, file) in [
        ("graphSha256", "graph.json"),
        ("overlaySha256", "rust.json"),
    ] {
        require(
            reason["baselineRecovery"][key] == sha(&portable().join(super::REASON_DIR).join(file))?,
            "linked choice differs from archived recovery",
        )?;
    }
    Ok(json!({
        "observations":{"baseline":baseline,"mutation":changed},
        "nativeReceiptSha256":sha(&native_path)?,
        "originalReferenceSha256":reference_sha,
        "linkedChoice":{"framework":reason["framework"],"receiptSha256":sha(&reason_path)?}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_cases_and_policy_change_are_independent_requirements() -> Result {
        let cases = load(&whole().join("gap-range-reference/cases.json"))?;
        let receipt = load(&whole().join("gap-range-record/aarch64-apple-darwin.json"))?;
        let baseline = list(&receipt["baseline"]["observations"])?;
        let original = list(&receipt["mutation"]["observations"])?;
        let cases = list(&cases)?;
        check_observations(cases, baseline, original)?;
        assert!(check_observations(cases, baseline, baseline).is_err());
        let mut rows = original.clone();
        rows.swap(0, 1);
        assert!(check_observations(cases, baseline, &rows).is_err());
        let mut rows = original.clone();
        rows.iter_mut()
            .find(|row| row["id"] == "unknown-field")
            .unwrap()["result"] = json!({"diagnostic":"different refusal"});
        assert!(check_observations(cases, baseline, &rows).is_err());
        let mut rows = original.clone();
        rows.iter_mut()
            .find(|row| row["id"] != "unknown-field")
            .unwrap()["result"] = Value::Null;
        assert!(check_observations(cases, baseline, &rows).is_err());
        Ok(())
    }
}
