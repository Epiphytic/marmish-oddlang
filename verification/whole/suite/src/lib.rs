//! Inspect archived native test and build observations without executing candidate software.
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Cases = BTreeMap<String, Vec<String>>;
const CORE: &str = "marmish-core/test/contracts";
const STREAM: &str = "marmish-stream/lib/marmish_stream";
const NATIVE: &str = "marmish-native/test/process_limits";
const ENGINE: &str = "marmish-engine/lib/marmish_engine";
const VAULT: &str = "marmish-vault/lib/marmish_vault";
const PROTOTYPE: &str = "marmish-prototype/lib/marmish_prototype";
const MDK: &str = "marmish-mdk/lib/marmish_mdk";
const PARSER: &str = "marmish-native/lib/marmish_native";

pub fn suite() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
pub fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn obj(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().ok_or_else(|| "expected object".into())
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| "expected string".into())
}
fn names(v: &Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value(v.clone())?)
}
fn same_keys(a: &Value, b: &Value) -> Result<bool> {
    Ok(obj(a)?.keys().collect::<BTreeSet<_>>() == obj(b)?.keys().collect::<BTreeSet<_>>())
}
fn matches(actual: &Value, expected: &Value, message: &str) -> Result<()> {
    if actual != expected {
        return Err(message.into());
    }
    Ok(())
}
#[derive(Deserialize)]
struct Candidate {
    manifest: String,
    compiler: String,
    kind: String,
}

struct Selected {
    name: String,
    path: PathBuf,
    kind: String,
}
fn select(receipt: &Value, rules: &Value) -> Result<Selected> {
    let root = suite().join("../../../oddlang/portable");
    let mut selected = Vec::new();
    let candidates: BTreeMap<String, Candidate> =
        serde_json::from_value(rules["candidates"].clone())?;
    for (name, c) in candidates {
        let path = root.join(c.manifest);
        if receipt["framework"] == c.compiler && receipt["manifestSha256"] == sha(&fs::read(&path)?)
        {
            selected.push(Selected {
                name,
                path,
                kind: c.kind,
            });
        }
    }
    if selected.len() != 1 {
        return Err("unknown workspace or compiler revision".into());
    }
    Ok(selected.remove(0))
}

fn inspect_selected(receipt: &Value, rules: &Value, c: &Selected) -> Result<Value> {
    let host = string(&receipt["target"])?;
    if !matches!(host, "aarch64-apple-darwin" | "aarch64-unknown-linux-gnu") {
        return Err("unknown native target".into());
    }
    let data = fs::read(suite().join(format!("named-release-{host}.json")))?;
    let catalog: Value = serde_json::from_slice(&data)?;
    let lock_bytes = fs::read(c.path.with_extension("").join("workspace.lock"))?;
    let lock: Value = serde_json::from_slice(&lock_bytes)?;
    if catalog["upstream"] != rules["upstream"]
        || receipt["original"] != rules["upstream"]
        || receipt["profile"] != catalog["selection"]
        || receipt["inventorySha256"] != sha(&data)
        || receipt["workspaceLockSha256"] != sha(&lock_bytes)
    {
        return Err("candidate or upstream profile seal differs".into());
    }
    let mdk = c.kind == "manifest-mdk";
    let parser = mdk || c.kind == "manifest";
    let reader = parser || c.kind == "seven";
    if mdk {
        for (key, expected) in [
            ("mdkZeroTestControl", &rules["mdkControl"]),
            ("mdkDeniedSourcePaths", &rules["mdkDenied"]),
            ("mdkNormalDependencies", &json!([])),
        ] {
            matches(
                &receipt[key],
                expected,
                "missing MDK runtime or zero-test control",
            )?;
        }
    } else if [
        "mdkZeroTestControl",
        "mdkDeniedSourcePaths",
        "mdkNormalDependencies",
    ]
    .iter()
    .any(|k| receipt.get(k).is_some())
    {
        return Err("unexpected MDK control".into());
    }
    if reader {
        let built = &receipt["builtLibraries"];
        if !same_keys(built, &lock["members"])?
            || !obj(built)?.values().all(|s| {
                s.as_str().is_some_and(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            })
        {
            return Err("missing or invalid native workspace library artifacts".into());
        }
    } else if receipt.get("builtLibraries").is_some() {
        return Err("unexpected workspace library evidence".into());
    }
    let mut passed = Cases::new();
    let mut sources = rules["sourceHashes"].clone();
    if c.kind != "v7" {
        let mut engine = vec![string(&rules["engineCase"])?.to_owned()];
        if host.ends_with("linux-gnu") {
            engine.push(string(&rules["linuxCase"])?.to_owned());
            engine.sort();
        }
        passed.insert(ENGINE.into(), engine);
        passed.insert(VAULT.into(), vec![string(&rules["vaultCase"])?.into()]);
        if reader {
            passed.insert(PROTOTYPE.into(), vec![string(&rules["readerCase"])?.into()]);
            sources["prototypeReader"] = rules["readerHash"].clone();
        }
        if parser {
            passed.insert(PARSER.into(), names(&rules["parserCases"])?);
            sources
                .as_object_mut()
                .unwrap()
                .extend(obj(&rules["parserHashes"])?.clone());
        }
        if mdk {
            passed.insert(MDK.into(), vec![string(&rules["mdkCase"])?.into()]);
            sources["mdkSyntheticJson"] = rules["mdkHash"].clone();
        }
        let mut counts = json!({});
        for (harness, cases) in &passed {
            for name in cases {
                counts[harness][name] = json!(if harness == ENGINE { 2 } else { 1 });
            }
        }
        matches(
            &receipt["passingProcesses"],
            &counts,
            "partial test process evidence differs",
        )?;
        matches(
            &receipt["selectedTestSources"],
            &sources,
            "partial harness source or observed cases disagree",
        )?;
    } else if !receipt["selectedTestSources"].is_null() || !receipt["passingProcesses"].is_null() {
        return Err("unexpected partial harness source or process evidence".into());
    }
    for harness in [CORE, STREAM, NATIVE] {
        let mut tests = names(&catalog["harnesses"][harness]["tests"])?;
        tests.sort();
        passed.insert(harness.into(), tests);
    }
    let native = &receipt["nativeProcessTest"];
    matches(
        &json!(passed[NATIVE]),
        &json!([native["test"]]),
        "process test name differs",
    )?;
    let prior = read(&suite().join(format!("../workspace/v7-{host}.json")))?;
    let mut modules = obj(&prior["testModules"])?.clone();
    modules.remove("exactCopyFrames");
    if prior["original"] != rules["upstream"]
        || receipt["upstreamModules"] != Value::Object(modules)
    {
        return Err("injected test modules differ from pinned suites".into());
    }
    for key in [
        "source",
        "source_sha256",
        "test",
        "upstream_commit",
        "compiler",
    ] {
        matches(
            &native[key],
            &prior["nativeProcessLimits"][key],
            "process test source or compiler differs",
        )?
    }
    let roles = json!({"marmish":null,"marmishd":null});
    if native["executables"] != receipt["releaseBinarySha256"]
        || !same_keys(&native["executables"], &roles)?
    {
        return Err("missing generated process role".into());
    }
    if native["staged_pair_exit"] != 0 || !same_keys(&native["mutants"], &roles)? {
        return Err("native executable witness failed its control".into());
    }
    for case in obj(&native["mutants"])?.values() {
        if case["exit"] != 101
            || case["failed_assertion"] != "!result.stdout.is_empty()"
            || case["sha256"] != sha(b"#!/bin/sh\nexit 0\n")
        {
            return Err("native executable witness failed its control".into());
        }
    }
    let mut rows = json!({});
    let mut totals = BTreeMap::<&str, usize>::new();
    for (harness, entry) in obj(&catalog["harnesses"])? {
        let tests = names(&entry["tests"])?;
        let eligible: BTreeSet<_> = tests.iter().collect();
        let ignored = names(&entry["ignored"])?;
        let excluded: BTreeSet<_> = ignored.iter().collect();
        let names = passed.remove(harness).unwrap_or_default();
        let seen: BTreeSet<_> = names.iter().collect();
        if eligible.len() != tests.len()
            || excluded.len() != ignored.len()
            || !eligible.is_disjoint(&excluded)
            || seen.len() != names.len()
            || !seen.is_subset(&eligible)
        {
            return Err(format!("invalid cases for {harness}").into());
        }
        let row = json!({"passed":names,"unverified":eligible.difference(&seen).collect::<Vec<_>>(),"ignored":excluded,"benchmarks":entry["benchmarks"]});
        for key in ["benchmarks", "ignored", "passed", "unverified"] {
            *totals.entry(key).or_default() +=
                row[key].as_array().ok_or("invalid case list")?.len();
        }
        rows[harness] = row;
    }
    if !passed.is_empty() {
        return Err("missing qualified harness".into());
    }
    matches(
        &receipt["harnesses"],
        &rows,
        "case dispositions disagree with catalog",
    )?;
    let totals = json!(totals);
    matches(&receipt["totals"], &totals, "totals disagree with catalog")?;
    if c.name == "canonical-public-data" {
        if receipt["manifestSha256"]
            != "b0cebc91d550db8daba623333d09db2917eb2cd55874957912cd97a3f9eb5fc4"
            || receipt["workspaceLockSha256"]
                != "b50bd2a6896f7f253b4ed6005844f7cc48704dc3663568815df811e60b68f3ab"
        {
            return Err("unexpected combined public-data candidate input".into());
        }
        let baseline = fs::read(suite().join(format!("canonical-{host}.json")))?;
        if sha(&baseline) != rules["canonicalPins"][host]
            || receipt["canonicalNamedBaselineSha256"] != rules["canonicalPins"][host]
        {
            return Err("candidate canonical baseline pin differs".into());
        }
        let reference: Value = serde_json::from_slice(&baseline)?;
        matches(&rows, &reference["harnesses"], "named baseline differs")?;
        matches(
            &receipt["preRunRegisteredNames"],
            &json!({CORE:rows[CORE]["passed"], STREAM:rows[STREAM]["passed"]}),
            "missing exact pre-execution registration evidence",
        )?;
    } else if receipt.get("canonicalNamedBaselineSha256").is_some()
        || receipt.get("preRunRegisteredNames").is_some()
    {
        return Err("unexpected canonical baseline evidence".into());
    }
    Ok(totals)
}
pub fn inspect(receipt: &Value) -> Result<Value> {
    let rules = read(&suite().join("receipt-rules.json"))?;
    inspect_selected(receipt, &rules, &select(receipt, &rules)?)
}

type Target = (String, String, String);
fn target(package: &str, kind: &str, name: &str) -> Target {
    (package.into(), kind.into(), name.into())
}

pub fn candidate_targets(receipt: &Value) -> Result<Value> {
    let rules = read(&suite().join("receipt-rules.json"))?;
    let selected = select(receipt, &rules)?;
    inspect_selected(receipt, &rules, &selected)?;
    let built = obj(&receipt["builtLibraries"])
        .map_err(|_| "selected receipt has no library build evidence")?;
    let original = read(&suite().join("../../whole-marmish/inventory/inventory.json"))?;
    matches(
        &original["commit"],
        &receipt["original"],
        "upstream target inventory revision differs from suite",
    )?;
    let roots: Vec<_> = original["workspaces"]
        .as_array()
        .ok_or("missing workspaces")?
        .iter()
        .filter(|v| v["manifest"] == "Cargo.toml")
        .collect();
    let [root] = roots.as_slice() else {
        return Err("expected one upstream root workspace".into());
    };
    let mut expected = BTreeMap::new();
    for package in root["packages"].as_array().ok_or("missing packages")? {
        for t in package["targets"].as_array().ok_or("missing targets")? {
            let kinds = names(&t["kind"])?;
            if let [k] = kinds.as_slice()
                && matches!(k.as_str(), "bin" | "lib")
            {
                expected.insert(
                    target(string(&package["name"])?, k, string(&t["name"])?),
                    t.get("required_features")
                        .ok_or("missing required features")?,
                );
            }
        }
    }
    let mut declared = BTreeSet::new();
    let mut observed = BTreeSet::new();
    let mut owners = BTreeMap::new();
    let manifest = read(&selected.path)?;
    for (name, member) in obj(&manifest["members"])? {
        let pin = &member["manifest"];
        let bytes = fs::read(selected.path.parent().unwrap().join(string(&pin["path"])?))?;
        let package: Value = serde_json::from_slice(&bytes)?;
        if pin["sha256"] != sha(&bytes) || package["name"] != name.as_str() {
            return Err(format!("mismatched package manifest for {name}").into());
        }
        let lib = target(name, "lib", &name.replace('-', "_"));
        declared.insert(lib.clone());
        if built.contains_key(name) {
            observed.insert(lib);
        }
        if let Some(bins) = package.get("binaries") {
            for bin in bins.as_array().ok_or("invalid binaries")? {
                let spelling = string(&bin["name"])?;
                if owners.insert(spelling.to_string(), name).is_some() {
                    return Err(format!("ambiguous binary owner: {spelling}").into());
                }
                declared.insert(target(name, "bin", spelling));
            }
        }
    }
    for spelling in obj(&receipt["releaseBinarySha256"])?.keys() {
        observed.insert(target(
            owners.get(spelling).ok_or("unknown release binary")?,
            "bin",
            spelling,
        ));
    }
    let mut targets = Vec::new();
    for (t, features) in &expected {
        targets.push(json!({"package":t.0,"kind":t.1,"name":t.2,"requiredFeatures":features,"declared":declared.contains(t),"buildObserved":observed.contains(t)}));
    }
    let extra: Vec<_> = observed
        .iter()
        .filter(|t| !expected.contains_key(*t))
        .collect();
    Ok(
        json!({"upstream":original["commit"],"host":receipt["target"],"workspaceLockSha256":receipt["workspaceLockSha256"],"targets":targets,"extraObserved":extra}),
    )
}
