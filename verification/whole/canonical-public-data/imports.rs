//! Compile every frozen public import against the selected seven native libraries.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
};
const HOST: &str = "aarch64-apple-darwin";
const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
pub const COUNT: [(&str, usize); 7] = [
    ("core", 6),
    ("engine", 48),
    ("mdk", 5),
    ("native", 36),
    ("prototype", 0),
    ("stream", 35),
    ("vault", 10),
];
const LOCK: &str = "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4";
fn object(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    Ok(v.as_object().ok_or("expected an object")?)
}
pub fn probe(host: &str, role: &Value, work: &Path, body: &str) -> Result<Value> {
    fs::write(work.join("caller.rs"), body)?;
    let meta = Path::new(text(&role[0])?);
    let mut cmd = Command::new("rustc");
    cmd.current_dir(work)
        .args([
            "+1.97.1",
            "--edition",
            "2024",
            "--target",
            host,
            "--error-format=json",
            "--crate-name",
            "separate_caller",
            "--extern",
        ])
        .arg(format!("observed={}", meta.display()))
        .arg("-L")
        .arg(format!(
            "dependency={}",
            meta.parent().ok_or("invalid rmeta path")?.display()
        ))
        .arg("-L")
        .arg(format!("dependency={}", text(&role[3])?))
        .args(["caller.rs", "--crate-type", "lib", "--emit", "metadata"])
        .env("CARGO_NET_OFFLINE", "true");
    for (key, _) in env::vars_os() {
        if key.to_str().is_some_and(|s| {
            [
                "RUST",
                "CARGO_BUILD_",
                "CARGO_ENCODED_",
                "CARGO_PROFILE_",
                "CARGO_TARGET_",
            ]
            .iter()
            .any(|p| s.starts_with(p))
        }) {
            cmd.env_remove(key);
        }
    }
    let result = capture(&mut cmd, 600, None)?;
    let mut diagnostics = Vec::new();
    for line in std::str::from_utf8(&result.stderr)?
        .lines()
        .filter(|l| l.starts_with('{'))
    {
        let r: Value = serde_json::from_str(line)?;
        if r["level"] == "error" && !r["code"].is_null() {
            let code = text(&r["code"]["code"])?;
            require(
                ["E0432", "E0433", "E0603", "ambiguous_glob_imports"].contains(&code),
                format!("unexpected import diagnostic: {code}"),
            )?;
            diagnostics.push(json!({"code":code,"message":r["message"],"rendered":r["rendered"]}));
        }
    }
    require(
        result.status.success() == diagnostics.is_empty(),
        "import status and diagnostics disagree",
    )?;
    Ok(json!({"compiles":result.status.success(),"diagnostics":diagnostics}))
}
pub fn library(name: &str, count: usize, host: &str, role: &Value, original: &Path) -> Result<Value> {
    let root = whole()
        .join("public-types")
        .join(format!("original-{name}"));
    let graph_path = root.join("exports.json");
    let lock_path = root.join("exports.lock");
    let evidence_path = root.join("evidence.json");
    let graph = load(&graph_path)?;
    let lock = load(&lock_path)?;
    let pin = &lock["pin"];
    let evidence = load(&evidence_path)?;
    require(
        evidence["exportsSha256"] == lock["exportsSha256"]
            && evidence["discrepancies"] == json!([])
            && evidence["compilePositiveCount"] == count
            && pin["cargoLockSha256"] == sha(&original.join("Cargo.lock"))?
            && pin["packageManifestSha256"]
                == sha(&original.join(format!("crates/marmish-{name}/Cargo.toml")))?,
        "invalid inventory evidence",
    )?;
    require(
        (pin.get("sourceRevision") == Some(&Value::Null) || pin["sourceRevision"] == ORIGINAL)
            && lock["exportsSha256"] == sha(&graph_path)?
            && graph["vocabulary"] == "oddlang-public-type-bindings-v1"
            && lock["vocabulary"] == graph["vocabulary"],
        "unexpected source inventory",
    )?;
    require(
        pin["package"] == format!("marmish-{name}")
            && pin["targetName"] == format!("marmish_{name}")
            && pin["compilerCommit"] == COMPILER
            && pin["targetTriple"] == HOST
            && pin["targetKind"] == "lib"
            && pin["defaultFeatures"] == true
            && pin["features"] == json!([]),
        "unexpected inventory profile",
    )?;
    let mut paths = BTreeMap::new();
    let mut unresolved: Vec<Vec<String>> = Vec::new();
    for m in object(&graph["modules"])?.values() {
        require(
            m["ambiguous"] == json!({}) || m["ambiguous"] == json!([]),
            "ambiguous module inventory",
        )?;
        let prefix: Vec<String> = serde_json::from_value(m["path"].clone())?;
        for item in list(&m["unresolved"])? {
            let mut path = prefix.clone();
            path.push(text(item)?.into());
            unresolved.push(path);
        }
        for (item, binding) in object(&m["bindings"])? {
            let mut path = prefix.clone();
            path.push(item.clone());
            require(
                paths.insert(path, text(&binding["kind"])?).is_none(),
                "duplicate declaration path",
            )?;
        }
    }
    require(
        paths.len() == count,
        "incomplete downstream import inventory",
    )?;
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    require(
        probe(host, role, work, "pub use observed as subject;")?["compiles"] == true,
        "extern crate probe failed",
    )?;
    let mut results = Vec::new();
    let mut available = 0;
    for (path, kind) in paths {
        let spelling = format!(
            "observed::{}",
            path.iter()
                .map(|s| format!("r#{s}"))
                .collect::<Vec<_>>()
                .join("::")
        );
        let r = probe(
            host,
            role,
            work,
            &format!("#![deny(ambiguous_glob_imports)]\npub use {spelling} as witness;\n"),
        )?;
        if r["compiles"] == true {
            available += 1;
        }
        results.push(json!({"spelling":path.join("::"),"originalInventoryKind":kind,"compiles":r["compiles"],"diagnostics":r["diagnostics"]}));
    }
    unresolved.sort();
    Ok(json!({
        "inventorySha256": sha(&graph_path)?,
        "inventoryLockSha256": sha(&lock_path)?,
        "inventoryUnresolved": unresolved,
        "inventoryEvidenceSha256": sha(&evidence_path)?,
        "metadataSha256": sha(Path::new(text(&role[0])?))?,
        "librarySha256": sha(Path::new(text(&role[1])?))?,
        "features": role[2],
        "cargoFresh": role[4],
        "importable": available,
        "missing": count-available,
        "imports": results,
    }))
}
pub fn observe(host: &str, original: &Path, work: &Path, roles: &Value) -> Result<Value> {
    let previous = if host == HOST {
        let archive = load(&whole().join("canonical-workspace/aarch64-apple-darwin.json"))?;
        require(
            archive["workspaceLockSha256"] == LOCK,
            "wrong canonical Darwin evidence",
        )?;
        archive["darwinPublicImports"]["libraries"].clone()
    } else {
        require(
            host == "aarch64-unknown-linux-gnu",
            "unsupported import host",
        )?;
        let archive = load(&whole().join(format!("canonical-linux-imports/{host}.json")))?;
        require(
            archive["candidate"]
                == json!({
                    "release": "workspace-v7-canonical-offline",
                    "input_revision": "0f9e57396530fa1c11127e3fff7d8d0018514f00",
                    "framework_revision": "8a20ea283d0d43da42f054a7c7774eb9e6a20da7",
                    "lock_sha256": LOCK,
                }),
            "wrong canonical Linux evidence",
        )?;
        archive["generatedBuild"]["libraries"].clone()
    };
    let mut libs = json!({});
    let mut gains = BTreeSet::new();
    let (mut old_count, mut new_count, mut attempted) = (0, 0, 0);
    for (name, count) in COUNT {
        let fresh = library(
            name,
            count,
            host,
            &roles[format!("marmish-{name}")],
            original,
        )?;
        let old = &previous[name];
        for key in [
            "inventorySha256",
            "inventoryLockSha256",
            "inventoryEvidenceSha256",
        ] {
            require(fresh[key] == old[key], "public import inventory changed")?;
        }
        let first = list(&old["imports"])?;
        let last = list(&fresh["imports"])?;
        require(first.len() == last.len(), "public import count changed")?;
        for (a, b) in first.iter().zip(last) {
            let spelling = text(&a["spelling"])?;
            require(
                b["spelling"] == spelling
                    && a["originalInventoryKind"] == b["originalInventoryKind"],
                "public import identity changed",
            )?;
            let old = a["compiles"].as_bool().ok_or("missing prior outcome")?;
            let new = b["compiles"].as_bool().ok_or("missing current outcome")?;
            attempted += 1;
            old_count += usize::from(old);
            new_count += usize::from(new);
            let diagnostics = |r: &Value| -> Result<Vec<(String, String)>> {
                list(&r["diagnostics"])?
                    .iter()
                    .map(|d| Ok((text(&d["code"])?.into(), text(&d["message"])?.into())))
                    .collect()
            };
            if new && !old {
                let codes = diagnostics(a)?;
                require(
                    codes.iter().map(|v| v.0.as_str()).eq(["E0432"]),
                    "unexpected prior error for new import",
                )?;
                gains.insert((name.to_owned(), spelling.to_owned()));
            } else {
                require(
                    old == new && diagnostics(a)? == diagnostics(b)?,
                    format!("inherited import outcome changed: {name}::{spelling}"),
                )?;
            }
        }
        libs[name] = fresh;
    }
    let added = [
        ("engine", "MissingReason"),
        ("engine", "Stream"),
        ("engine", "SubmitDisposition"),
        ("native", "mdk_adapter::RuntimeStatus"),
    ]
    .map(|(a, b)| (a.to_owned(), b.to_owned()));
    require(
        attempted == 140
            && old_count == 77
            && new_count == 81
            && gains.into_iter().eq(added.clone()),
        "unexpected public import totals or gains",
    )?;
    let mut controls = json!({});
    for (case, spelling, code) in [
        ("absent", "NoSuchAuditSentinel", "E0432"),
        ("private", "config", "E0603"),
    ] {
        let actual = probe(
            host,
            &roles["marmish-core"],
            work,
            &format!("pub use observed::{spelling};"),
        )?;
        require(
            actual["compiles"] == false
                && list(&actual["diagnostics"])?.len() == 1
                && actual["diagnostics"][0]["code"] == code,
            format!("unexpected native import control: {case}"),
        )?;
        controls[case] = actual;
    }
    Ok(
        json!({"attempted":attempted,"baseline":old_count,"candidate":new_count,"added":added,"controls":controls,"libraries":libs}),
    )
}
