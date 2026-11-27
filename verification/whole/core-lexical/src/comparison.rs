use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Key = (String, Vec<String>);

fn reports(host: &str) -> Result<[Value; 2]> {
    let dir = lexical().join(host);
    let receipt = load(&dir.join("receipt.json"))?;
    let before = &receipt["before"];
    require(
        receipt["host"] == host
            && *before == receipt["after"]
            && before["workspaceLockSha256"] == LOCK,
        "receipt host or provenance mismatch",
    )?;
    require(
        receipt["observerRevision"] == LEXICAL_OBSERVER
            && receipt["originalRevision"] == ORIGINAL
            && receipt["projectionRevision"] == PROJECTION,
        "receipt revision mismatch",
    )?;
    require(
        receipt["mutation"]
            == json!({"file":"src/lib.rs","outcome":"generated file changed: src/lib.rs"}),
        "missing changed-input rejection",
    )?;
    require(sha(&workspace_lock())? == LOCK, "canonical lock changed")?;
    let ws = load(&workspace_lock())?;
    let member = &ws["members"]["marmish-core"];
    require(
        sha(&package())? == txt(&member["inputSha256"])?,
        "core package input changed",
    )?;
    let lockfile = lexical().join("package.lock");
    let lock = load(&lockfile)?;
    let digest = sha(&lockfile)?;
    require(
        member["standalonePackageLockSha256"] == digest
            && txt(&before["coreManifestSha256"])? == txt(&member["inputSha256"])?
            && txt(&member["inputSha256"])? == txt(&lock["manifestSha256"])?,
        "wrong standalone package lock or input",
    )?;
    require(
        before["generatedFiles"] == sealed_files(&lock, &digest)?,
        "receipt inventory disagrees with the sealed package",
    )?;
    let fetch = |role: &str| -> Result<Value> {
        let file = dir.join(format!("{role}.json"));
        require(
            receipt["reportsSha256"][role] == sha(&file)?,
            "archived report digest changed",
        )?;
        let r = load(&file)?;
        require(
            r["tools"]["host"] == host
                && r["profile"]["target"] == json!([host])
                && r["package"] == "marmish-core"
                && r["targetKind"] == "lib"
                && r["cargoProfile"] == "release"
                && r["defaultFeatures"] == true
                && r["requestedFeatures"] == json!([]),
            "observer was not a native release build",
        )?;
        if role == "original" {
            require(
                r["source"]["revision"] == ORIGINAL,
                "wrong original revision",
            )?;
        } else {
            let files = &before["generatedFiles"];
            require(
                r.get("source") == Some(&Value::Null)
                    && r["workspaceManifestSha256"] == files["Cargo.toml"]
                    && r["cargoLockSha256"] == files["Cargo.lock"],
                "wrong generated inputs",
            )?;
        }
        Ok(r)
    };
    Ok([fetch("original")?, fetch("generated")?])
}
fn summary(r: &Value) -> Result<Value> {
    let data = &r["comparison"];
    let access = &data["declaredAccess"];
    Ok(json!({
        "unambiguous":arr(&data["matched"])?.len(),"ambiguous":arr(&data["ambiguous"])?.len(),
        "compilerOnly":arr(&data["compilerOnly"])?.len(),"rustdocOnly":arr(&data["rustdocOnly"])?.len(),
        "visibilityAgreements":arr(&access["agreement"])?.len(),"visibilityDisagreements":arr(&access["different"])?.len(),
        "visibilityUnresolved":arr(&access["unresolved"])?.len(),"outsideVisibilityScopeByKind":access["outsideScopeMatchedByKind"]
    }))
}
fn declarations(r: &Value) -> Result<BTreeMap<Key, &Value>> {
    arr(&r["comparison"]["matched"])?
        .iter()
        .map(|row| {
            Ok((
                (
                    txt(&row["kind"])?.into(),
                    serde_json::from_value(row["path"].clone())?,
                ),
                row,
            ))
        })
        .collect()
}
pub fn compare() -> Result<Value> {
    let darwin = reports(HOSTS[0])?;
    let linux = reports(HOSTS[1])?;
    for (a, b) in darwin.iter().zip(&linux) {
        require(
            a["comparison"] == b["comparison"],
            "different native declaration evidence across hosts",
        )?;
    }
    let original = declarations(&darwin[0])?;
    let generated = declarations(&darwin[1])?;
    let canonical = whole().join("canonical-workspace/aarch64-apple-darwin.json");
    let c = load(&canonical)?;
    require(
        c["workspaceLockSha256"] == LOCK,
        "wrong downstream audit workspace",
    )?;
    let imports = &c["darwinPublicImports"]["libraries"]["core"];
    let inventory = whole().join("public-types/original-core/exports.json");
    let inv_hash = sha(&inventory)?;
    require(
        imports["inventorySha256"] == inv_hash,
        "wrong original export inventory",
    )?;
    let inv = load(&inventory)?;
    let bindings = object(&inv["modules"][txt(&inv["root"])?]["bindings"])?;
    let imported: BTreeSet<_> = arr(&imports["imports"])?
        .iter()
        .map(|r| txt(&r["spelling"]))
        .collect::<Result<_>>()?;
    require(
        bindings
            .keys()
            .map(String::as_str)
            .eq(imported.iter().copied())
            && bindings.len() == 6
            && imports["importable"] == 6
            && imports["missing"] == 0,
        "incomplete Darwin import audit",
    )?;
    let package = load(&package())?;
    let lock = load(&lexical().join("package.lock"))?;
    for (artifact, source) in [
        ("crate-root.rs.txt", "src/lib.rs"),
        ("config.rs.txt", "src/group_0/mod.rs"),
        ("frame.rs.txt", "src/group_2/mod.rs"),
    ] {
        require(
            lock["generatedFiles"][source] == sha(&lexical().join("export-facade").join(artifact))?,
            "archived re-export source disagrees with the sealed package",
        )?;
    }
    let mut rows = Vec::new();
    for observed in arr(&imports["imports"])? {
        let name = txt(&observed["spelling"])?;
        require(
            observed["compiles"] == true
                && observed["originalInventoryKind"] == bindings[name]["kind"]
                && bindings[name]["kind"] == "data",
            format!("missing public data import {name}"),
        )?;
        let (pos, component) =
            one(arr(&package["components"])?
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    r["reexports"]
                        .as_array()
                        .is_some_and(|list| list.iter().any(|s| s == name))
                }))?;
        let sealed = &lock["components"][pos];
        require(
            ["kind", "module", "reexports"]
                .iter()
                .all(|k| sealed[k] == component[k])
                && sealed["graphSha256"] == component["graph"]["sha256"]
                && sealed["overlaySha256"] == component["overlay"]["sha256"],
            "component pins or public bindings disagree with package lock",
        )?;
        let (file, _) = one(object(&lock["generatedFiles"])?
            .iter()
            .filter(|(_, d)| **d == sealed["sourceSha256"]))?;
        let module = txt(&component["module"])?;
        let old = one(original
            .keys()
            .filter(|(_, p)| p.as_slice() == ["marmish_core", module, name]))?;
        let binding = txt(&bindings[name]["target"])?;
        require(
            old.1.join("::")
                == binding
                    .strip_prefix("local::")
                    .unwrap_or(binding)
                    .replace("::{type}", ""),
            "original binding has another owner",
        )?;
        let new = one(generated.keys().filter(|(kind, p)| {
            *kind == old.0 && p.get(..2) == old.1.get(..2) && p.last().is_some_and(|s| s == name)
        }))?;
        require(
            old.1 != new.1,
            "expected different native declaration paths",
        )?;
        for side in ["compiler", "rustdoc"] {
            require(
                one(arr(&generated[new][side])?.iter())?["file"] == file.as_str(),
                "generated type is outside its sealed component",
            )?;
        }
        for (key, report) in [old, new].into_iter().zip(&darwin) {
            require(
                arr(&report["comparison"]["declaredAccess"]["agreement"])?
                    .iter()
                    .any(|r| {
                        r["kind"] == key.0
                            && r["path"] == json!(key.1)
                            && r["compiler"] == "public"
                            && r["rustdoc"] == "public"
                    }),
                "unexpected declared visibility",
            )?;
        }
        rows.push(json!({
            "spelling":name,"nativeRole":"crate-root public data reexport","normalizedKind":old.0,
            "originalDeclarationPath":old.1,"generatedDeclarationPath":new.1,"generatedSourceFile":file,"declaredAccessInBoth":"public",
            "component":{"kind":component["kind"],"module":component["module"],"graph":component["graph"],"overlay":component["overlay"]}
        }));
    }
    let same: Vec<_> = original
        .keys()
        .filter(|k| generated.contains_key(*k))
        .map(|(kind, path)| json!({"kind":kind,"path":path}))
        .collect();
    Ok(json!({
        "scope":"lexical declarations at a selected native compiler profile","observedHosts":HOSTS,
        "original":summary(&darwin[0])?,"generated":summary(&darwin[1])?,"sameLexicalKeys":same,"publicDataRolePairs":rows,
        "darwinDownstreamImportAudit":{"canonicalReceiptSha256":sha(&canonical)?,"inventorySha256":inv_hash,"compiled":6,"attempted":6}
    }))
}
