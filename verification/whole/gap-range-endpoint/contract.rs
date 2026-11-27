//! Check the pinned range endpoint contract and stage its representation-hint variants.
use public_data_qualification::{
    Result, assembly, json_bytes, list, load, pin, portable, product, require, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const ID: &str = "urn:marmish:engine:output-gap:range:end";
const MANIFEST: &str = "engine-gap-range-native-idioms-package.json";
const SEALED: &str = "6355ae8504a7eb64ae01fc0591f29e3879c38128";

fn bundle() -> PathBuf {
    portable().join("engine-gap-range-native-idioms-v1")
}
fn cases_path() -> PathBuf {
    whole().join("gap-range-endpoint/cases.json")
}

fn checked(base: &Path, descriptor: &Value) -> Result<PathBuf> {
    let name = text(&descriptor["path"])?;
    require(
        Path::new(name)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "expected a relative input path",
    )?;
    let path = base.join(name).canonicalize()?;
    require(
        path.starts_with(base.canonicalize()?) && pin(base, name)? == *descriptor,
        format!("input pin differs: {name}"),
    )?;
    Ok(path)
}

fn semantics(wire: &Value, graph: &Value, binding: &Value, cases: &Value) -> Result<()> {
    require(
        wire["role"] == "json-wire" && wire["pathBase"] == "companion-repository-root",
        "unexpected contract role or path base",
    )?;
    let mut matches = Vec::new();
    for module in list(&graph["modules"])? {
        for record in list(&module["records"])? {
            matches.extend(list(&record["fields"])?.iter().filter(|f| f["id"] == ID));
        }
    }
    require(
        matches.as_slice() == [&json!({"id":ID,"wire":"end","type":{"optional":"u64"}})],
        "portable endpoint graph differs",
    )?;
    require(
        wire["endpoint"]
            == json!({"id":ID,"absent":"open","null":"open","integer":{"minimum":"0","maximum":"18446744073709551615"},"encodeOpen":null,"encodePresent":"exact-json-integer"}),
        "portable endpoint wire contract differs",
    )?;
    require(
        binding["sourceApi"]
            == json!({"role":"rust-drop-in-source-api","fields":{(ID):{"name":"end","visibility":"public","type":{"optional":"u64"}}}}),
        "Rust source-API requirement differs",
    )?;
    let exact = |v: &Value| -> bool {
        v.as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .is_some_and(|v| v["end"].as_u64() == Some(9_007_199_254_740_993))
    };
    require(
        list(cases)?
            .iter()
            .any(|c| exact(&c["wire"]) && exact(&c["expected"])),
        "missing exact precision-loss case",
    )
}

pub fn validate() -> Result<Value> {
    let root = assembly::root();
    let wire = load(&bundle().join("wire.json"))?;
    let graph = load(&checked(root, &wire["graph"])?)?;
    let cases = cases_path();
    require(
        wire["cases"]
            == pin(
                root,
                cases
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("invalid cases path")?,
            )?,
        "portable corpus pin differs",
    )?;
    let cases = load(&cases)?;
    semantics(&wire, &graph, &load(&bundle().join("rust.json"))?, &cases)?;
    Ok(cases)
}

pub fn stage(work: &Path, profile: &str) -> Result<Value> {
    require(
        [
            "baseline",
            "replacement",
            "without-rust",
            "without-typescript",
            "absent",
            "negative-binding",
        ]
        .contains(&profile),
        "unknown hint profile",
    )?;
    let lane = work.join(profile);
    let inputs = lane.join("inputs");
    let wire = bundle().join("wire.json");
    let graph = checked(assembly::root(), &load(&wire)?["graph"])?;
    product::pinned(&graph, SEALED)?;
    let spec = load(&portable().join(MANIFEST))?;
    fs::create_dir_all(&inputs)?;
    let copy = |descriptor: &Value| -> Result<()> {
        checked(&portable(), descriptor)?;
        assembly::pinned_copy(&portable(), descriptor, &inputs)?;
        Ok(())
    };
    copy(&spec["cargoLock"])?;
    for component in list(&spec["components"])? {
        copy(&component["graph"])?;
        copy(&component["overlay"])?;
    }
    fs::copy(wire, lane.join("wire.json"))?;
    fs::copy(cases_path(), lane.join("cases.json"))?;
    let file = bundle().join("hints.json");
    if profile != "absent" && file.is_file() {
        let mut hints = load(&file)?;
        let rows = hints["hints"].as_array_mut().ok_or("invalid hints list")?;
        if profile == "replacement" {
            for row in rows.iter_mut() {
                match text(&row["language"])? {
                    "rust" => row["representation"] = json!("OpenOr<u64>"),
                    "typescript" => row["representation"] = json!("decimal string | null"),
                    _ => {}
                }
            }
        } else if let Some(language) = profile.strip_prefix("without-") {
            rows.retain(|row| row["language"] != language);
        }
        hints.sort_all_objects();
        fs::write(lane.join("hints.json"), json_bytes(&hints)?)?;
    }
    let manifest = inputs.join(MANIFEST);
    fs::copy(portable().join(MANIFEST), &manifest)?;
    Ok(json!([lane, manifest]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn role_and_precision_requirements_are_independent_of_hints() -> Result<()> {
        let wire = load(&bundle().join("wire.json"))?;
        let graph = load(&checked(assembly::root(), &wire["graph"])?)?;
        let binding = load(&bundle().join("rust.json"))?;
        let cases = load(&cases_path())?;
        semantics(&wire, &graph, &binding, &cases)?;
        let mut invalid = wire.clone();
        invalid["endpoint"]["integer"]["maximum"] = json!("9007199254740991");
        assert!(semantics(&invalid, &graph, &binding, &cases).is_err());
        let mut ambiguous = graph.clone();
        ambiguous["modules"]
            .as_array_mut()
            .unwrap()
            .extend(list(&graph["modules"])?.clone());
        assert!(semantics(&wire, &ambiguous, &binding, &cases).is_err());
        let mut private = binding.clone();
        private["sourceApi"]["fields"][ID]["visibility"] = json!("private");
        assert!(semantics(&wire, &graph, &private, &cases).is_err());
        for value in [json!(9007199254740992u64), json!("9007199254740993")] {
            let rows = json!([{"wire":"{\"end\":9007199254740993}", "expected":json!({"end":value}).to_string()}]);
            assert!(semantics(&wire, &graph, &binding, &rows).is_err());
        }
        Ok(())
    }
}
