//! Bind archived public export and caller evidence to the original and regenerated source trees.
use clap::{Parser, ValueEnum};
use public_data_qualification::{assembly::checkout, product::check_members, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const TARGET: &str = "aarch64-apple-darwin";
#[derive(Clone, Copy, ValueEnum)]
enum Release {
    V5,
    V6,
}
#[derive(Parser)]
struct Args {
    upstream: PathBuf,
    generated: PathBuf,
    rust_exports: PathBuf,
    #[arg(long, value_enum)]
    release: Release,
}

fn obj(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    Ok(v.as_object().ok_or("expected object")?)
}
fn keys(v: &Value) -> Result<BTreeSet<&str>> {
    Ok(obj(v)?.keys().map(String::as_str).collect())
}

fn manifests(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let cargo: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    let entries = cargo
        .get("workspace")
        .and_then(|v| v.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or("missing workspace members")?;
    let mut paths = BTreeMap::new();
    for entry in entries {
        let dir = root.join(entry.as_str().ok_or("expected member path")?);
        let path = dir.join("Cargo.toml");
        let package: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
        let name = package
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .ok_or("missing package name")?;
        let library = package
            .get("lib")
            .and_then(|v| v.get("path"))
            .map(|p| p.as_str().ok_or("invalid library path"))
            .transpose()?
            .unwrap_or("src/lib.rs");
        require(
            dir.join(library).is_file() && paths.insert(name.to_owned(), path).is_none(),
            format!("missing library or duplicate package {name}"),
        )?;
    }
    Ok(paths)
}

fn generated_members(root: &Path, release: &str, pin: &Value) -> Result<BTreeSet<String>> {
    let path = portable().join(format!("workspace-{release}"));
    let lock = load(&path.join("workspace.lock"))?;
    let manifest = path.with_extension("json");
    require(
        sha(&path.join("workspace.lock"))? == text(&pin["workspaceLockSha256"])?
            && fs::read(root.join("workspace.lock"))? == fs::read(path.join("workspace.lock"))?,
        "wrong generated workspace lock",
    )?;
    require(
        sha(&manifest)? == text(&lock["inputSha256"])?
            && keys(&load(&manifest)?["members"])? == keys(&lock["members"])?
            && keys(&pin["members"])? == keys(&lock["members"])?,
        "release manifest or provenance member set differs",
    )?;
    check_members(root, &lock, false)?;
    for (name, member) in obj(&lock["members"])? {
        require(
            !root.join(name).join("build.rs").try_exists()?
                && member["standalonePackageLockSha256"] == pin["members"][name],
            format!("unexpected build script or package lock for {name}"),
        )?;
    }
    Ok(obj(&lock["members"])?.keys().cloned().collect())
}

fn profile(
    folder: &Path,
    spec: &Value,
    name: &str,
    workspace: &Path,
    manifest: &Path,
) -> Result<Value> {
    for (file, key) in [
        ("exports.json", "exportsSha256"),
        ("exports.lock", "lockSha256"),
        ("evidence.json", "evidenceSha256"),
    ] {
        require(
            sha(&folder.join(file))? == text(&spec[key])?,
            format!("digest mismatch for {}", folder.join(file).display()),
        )?;
    }
    let lock = load(&folder.join("exports.lock"))?;
    let e = load(&folder.join("evidence.json"))?;
    let p = &lock["pin"];
    let package_sha = sha(manifest)?;
    require(
        p["package"] == name
            && e["package"] == name
            && p["targetKind"] == "lib"
            && p["packageManifestSha256"] == package_sha
            && (p["manifestSha256"] == package_sha
                || p["manifestSha256"] == sha(&workspace.join("Cargo.toml"))?)
            && p["cargoLockSha256"] == sha(&workspace.join("Cargo.lock"))?
            && lock["exportsSha256"] == spec["exportsSha256"]
            && e["exportsSha256"] == spec["exportsSha256"]
            && e["deterministic"] == true
            && [&e, p]
                .iter()
                .all(|v| v["defaultFeatures"] == true && v["features"] == json!([]))
            && p["targetTriple"] == TARGET
            && e["target"] == TARGET
            && text(&p["compilerCommit"])? == text(&e["compiler"])?,
        format!("wrong profile or input {}", folder.display()),
    )?;
    let fields = &e["fieldProbes"];
    Ok(
        json!({"profile":folder.file_name().and_then(|s|s.to_str()).ok_or("invalid profile name")?,"callerSpellings":e["compilePositiveCount"],"rustdocSpellings":e["rustdocSpellingCount"],
        "discrepancies":list(&e["discrepancies"])?.len(),"publicReads":fields["publicReads"],"restrictedRejections":fields["privateRejections"],"fieldProbeGaps":list(&fields["gaps"])?.len()}),
    )
}

type Names = BTreeMap<Vec<String>, String>;
fn exports(path: &Path) -> Result<(Names, Vec<Vec<String>>)> {
    let graph = load(path)?;
    let mut names = Names::new();
    let mut unresolved = Vec::new();
    for row in obj(&graph["modules"])?.values() {
        let module: Vec<String> = serde_json::from_value(row["path"].clone())?;
        let spelling = |name: &str| {
            let mut result = module.clone();
            result.push(name.into());
            result
        };
        for (name, edge) in obj(&row["bindings"])? {
            require(
                names
                    .insert(spelling(name), text(&edge["kind"])?.into())
                    .is_none(),
                "duplicate export spelling",
            )?;
        }
        for name in list(&row["unresolved"])? {
            unresolved.push(spelling(text(name)?));
        }
    }
    unresolved.sort();
    Ok((names, unresolved))
}

fn caller_counts(receipt: &Value, expected: &Path, subject: &Path, package: &str) -> Result<Value> {
    let subject_lock = subject.with_file_name("exports.lock");
    let lock = load(&subject_lock)?;
    let (wanted, unresolved) = exports(expected)?;
    let (found, _) = exports(subject)?;
    require(
        receipt["expectedExportsSha256"] == sha(expected)?
            && receipt["expectedLockSha256"] == sha(&expected.with_file_name("exports.lock"))?
            && receipt["expectedUnresolved"] == json!(unresolved)
            && receipt["subjectExportsSha256"] == sha(subject)?
            && receipt["subjectLockSha256"] == sha(&subject_lock)?
            && receipt["compiler"] == lock["pin"]["compilerCommit"]
            && receipt["subjectTargetName"] == lock["pin"]["targetName"]
            && receipt["subjectPackage"] == package
            && receipt["target"] == TARGET,
        "unbound caller evidence",
    )?;
    let rows = list(&receipt["imports"])?;
    require(
        rows.len() == wanted.len(),
        "missing or repeated import probes",
    )?;
    let mut compiled = 0;
    for (row, (path, kind)) in rows.iter().zip(wanted.iter()) {
        let actual = found.get(path);
        require(
            row["spelling"] == json!(path)
                && row["expectedKind"] == kind.as_str()
                && row.get("subjectKind") == Some(&json!(actual)),
            "wrong import spelling or namespace classification",
        )?;
        let passes = actual.is_some();
        let codes = if passes { json!([]) } else { json!(["E0432"]) };
        require(
            row["importCompiles"] == passes && row["errorCodes"] == codes,
            "wrong import result or error codes",
        )?;
        compiled += usize::from(passes);
    }
    Ok(json!({"compiled":compiled,"rejected":rows.len()-compiled}))
}

fn comparison(tool: &Path, old: &Path, new: &Path, archive: &Path) -> Result<Value> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("diff.json");
    run(Command::new(tool)
        .arg("compare")
        .arg(old)
        .arg(new)
        .arg(&out))?;
    require(
        fs::read(out)? == fs::read(archive)?,
        format!("comparison drift: {}", archive.display()),
    )?;
    let c = load(archive)?;
    require(
        c["originalSha256"] == sha(old)? && c["generatedSha256"] == sha(new)?,
        "comparison input mismatch",
    )?;
    Ok(
        json!({"originalOnly":list(&c["originalOnly"])?.len(),"generatedOnly":list(&c["generatedOnly"])?.len(),
        "changedAtSharedSpelling":list(&c["changedAtSharedSpelling"])?.len(),"shared":c["sharedCount"]}),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let archive = whole().join("public-types");
    let pins = load(&archive.join("provenance.json"))?;
    let (release, role, release_pin, generated_profiles) = match args.release {
        Release::V5 => ("v5", "generated", &pins["generated"], &pins["profiles"]),
        Release::V6 => (
            "v6",
            "generated-v6",
            &pins["generatedV6"],
            &pins["profilesV6"],
        ),
    };
    checkout(&args.upstream, text(&pins["original"]["revision"])?, false)?;
    let originals = manifests(&args.upstream)?;
    let members = generated_members(&args.generated, release, release_pin)?;
    require(
        members.iter().all(|m| originals.contains_key(m)),
        "unexpected generated member",
    )?;
    let profiles: BTreeMap<_, _> =
        obj(&pins["profiles"])?
            .iter()
            .filter(|(k, _)| k.starts_with("original-"))
            .chain(obj(generated_profiles)?.iter().filter(|(k, _)| {
                matches!(args.release, Release::V6) || k.starts_with("generated-")
            }))
            .collect();
    let mut observed = BTreeSet::new();
    let mut rows = Vec::new();
    for (name, manifest) in &originals {
        let suffix = name.strip_prefix("marmish-").unwrap_or(name);
        let original = format!("original-{suffix}");
        let record = |folder: &String, workspace: &Path, path: &Path| {
            profile(
                &archive.join(folder),
                profiles.get(folder).ok_or("missing pinned profile")?,
                name,
                workspace,
                path,
            )
        };
        let mut row = json!({"package":name,"original":record(&original,&args.upstream,manifest)?,"generated":null});
        let old = archive.join(&original).join("exports.json");
        observed.insert(original);
        if members.contains(name) {
            let generated = format!("{role}-{suffix}");
            row["generated"] = record(
                &generated,
                &args.generated,
                &args.generated.join(name).join("Cargo.toml"),
            )?;
            let new = archive.join(&generated).join("exports.json");
            observed.insert(generated);
            let tag = if matches!(args.release, Release::V6) {
                "-v6"
            } else {
                ""
            };
            row["comparison"] = comparison(
                &args.rust_exports,
                &old,
                &new,
                &archive.join(format!("{suffix}{tag}-comparison.json")),
            )?;
            row["originalImportsAgainstGenerated"] = caller_counts(
                &load(&archive.join(format!("{suffix}-callers-{release}.json")))?,
                &old,
                &new,
                name,
            )?;
            if name == "marmish-native" && matches!(args.release, Release::V6) {
                row["generatedImportsAgainstOriginal"] = caller_counts(
                    &load(&archive.join("native-callers-rev.json"))?,
                    &new,
                    &old,
                    name,
                )?;
            }
        }
        rows.push(row);
    }
    require(
        observed.iter().eq(profiles.keys().copied()),
        "unexpected or missing profiles",
    )?;
    let summary =
        json!({"release":release,"target":TARGET,"profile":"default release","packages":rows});
    require(
        summary == load(&archive.join(format!("workspace-{release}.json")))?,
        "workspace coverage summary drifted",
    )?;
    println!(
        "Verified {} original libraries against the {release} member set.",
        originals.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_count_cannot_substitute_for_a_bound_import() -> Result {
        let archive = whole().join("public-types");
        let expected = archive.join("original-core/exports.json");
        let subject = archive.join("generated-v6-core/exports.json");
        let good = load(&archive.join("core-callers-v6.json"))?;
        let check = |r: &Value| caller_counts(r, &expected, &subject, "marmish-core");
        assert_eq!(check(&good)?, json!({"compiled":6,"rejected":0}));
        let mut bad = good.clone();
        bad["imports"][1] = bad["imports"][0].clone();
        assert!(check(&bad).is_err());
        for (field, value) in [
            ("expectedLockSha256", json!("stale")),
            ("expectedUnresolved", json!([["ConfigError"]])),
            ("subjectPackage", json!("other")),
            ("target", json!("other")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(check(&bad).is_err(), "{field}");
        }
        let mut bad = good;
        bad["imports"][0]["subjectKind"] = json!("primitive");
        assert!(check(&bad).is_err());
        Ok(())
    }

    #[test]
    fn rejected_imports_require_the_pinned_diagnostic() -> Result {
        let archive = whole().join("public-types");
        let expected = archive.join("generated-v6-native/exports.json");
        let subject = archive.join("original-native/exports.json");
        let good = load(&archive.join("native-callers-rev.json"))?;
        let check = |r: &Value| caller_counts(r, &expected, &subject, "marmish-native");
        assert_eq!(check(&good)?, json!({"compiled":0,"rejected":2}));
        for (field, value) in [
            ("errorCodes", json!(["E0603"])),
            ("importCompiles", json!(true)),
            ("expectedKind", json!("data")),
        ] {
            let mut bad = good.clone();
            bad["imports"][0][field] = value;
            assert!(check(&bad).is_err(), "{field}");
        }
        Ok(())
    }
}
