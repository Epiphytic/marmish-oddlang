//! Exercise the published JSON endpoint cases against original and generated Rust libraries.
use clap::Parser;
use public_data_qualification::{
    artifacts,
    assembly::{ORIGINAL, bytes_sha, checkout},
    manifest_native,
    range::{AUTHORED, ENGINE, MODEL, hashed, host, keys, library_closure},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    original_target: PathBuf,
    retained: PathBuf,
    out: PathBuf,
}

fn command(tool: &Path, dir: &Path) -> Command {
    let mut cmd = Command::new(tool);
    cmd.current_dir(dir).env_clear();
    for key in [
        "HOME",
        "PATH",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "SDKROOT",
    ] {
        if let Some(value) = env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd
}

fn library(rows: &[Value], name: &str, release: &Path) -> Result<PathBuf> {
    let mut paths = BTreeSet::new();
    for row in rows.iter().filter(|r| {
        r["reason"] == "compiler-artifact"
            && r["target"]["name"] == name
            && r["target"]["kind"] == json!(["lib"])
            && r["profile"]["opt_level"] == "3"
    }) {
        for f in list(&row["filenames"])? {
            let p = Path::new(text(f)?);
            if p.starts_with(release) && p.extension().is_some_and(|e| e == "rlib") {
                paths.insert(p.to_owned());
            }
        }
    }
    require(
        paths.len() == 1,
        format!("expected one release {name} library"),
    )?;
    Ok(paths.pop_first().unwrap())
}

fn observe(
    raw: &[u8],
    engine: &Path,
    target: &Path,
    host: &str,
    work: &Path,
    vectors: &[u8],
) -> Result<Value> {
    let release = target.join(host).join("release");
    let rows = manifest_native::rows(raw)?;
    let serde = library(&rows, "serde_json", &release)?;
    let hashes = || -> Result<Value> {
        Ok(json!({"marmish_engine":sha(engine)?,"serde_json":sha(&serde)?}))
    };
    let before = hashes()?;
    let mut cmd = command(Path::new("rustc"), work);
    cmd.args([
        "+1.97.1",
        "--edition=2024",
        "--target",
        host,
        "--cfg",
        "with_range",
    ]);
    manifest_native::external(&mut cmd, "marmish_engine", engine);
    manifest_native::external(&mut cmd, "serde_json", &serde);
    manifest_native::depend(&mut cmd, &release.join("deps"));
    manifest_native::depend(&mut cmd, &target.join("release/deps"));
    let mut links = BTreeSet::new();
    for row in rows
        .iter()
        .filter(|r| r["reason"] == "build-script-executed")
    {
        for link in list(&row["linked_paths"])? {
            links.insert(text(link)?);
        }
    }
    for link in links {
        cmd.arg("-L").arg(link);
    }
    let exe = work.join("endpoint-probe");
    run(cmd
        .arg(whole().join("gap-range-reference/probe.rs"))
        .arg("-o")
        .arg(&exe))?;
    let observed = capture(&mut command(&exe, work), 60, Some(vectors))?;
    require(
        observed.status.success(),
        String::from_utf8_lossy(&observed.stderr),
    )?;
    require(
        before == hashes()?,
        "selected library changed during observation",
    )?;
    Ok(json!({"librarySha256":before,"results":serde_json::from_slice::<Value>(&observed.stdout)?}))
}

fn validate(cases: &[Value], results: &Value) -> Result {
    let rows = list(results)?;
    require(
        cases.len() == rows.len(),
        "wrong endpoint observation count",
    )?;
    let mut ids = BTreeSet::new();
    for (case, row) in cases.iter().zip(rows) {
        let id = text(&case["id"])?;
        require(
            ids.insert(id) && row["id"] == id,
            "duplicate or mismatched endpoint ID",
        )?;
        let expected = case
            .get("expected")
            .ok_or("missing expected endpoint outcome")?;
        let actual = &row["result"];
        let matches = if expected.is_null() {
            keys(actual)? == ["column", "diagnostic", "line"].map(String::from).into()
        } else {
            actual == &json!({"encoded":text(expected)?})
        };
        require(matches, format!("wrong endpoint observation for {id}"))?;
    }
    Ok(())
}

fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    let path = std::path::absolute(&a.original_target)?;
    let ancestor = path
        .ancestors()
        .find(|p| p.exists())
        .ok_or("missing target ancestor")?;
    require(
        !ancestor.canonicalize()?.starts_with(&original),
        "expected independent original Cargo target",
    )?;
    fs::create_dir_all(&path)?;
    let original_target = path.canonicalize()?;
    let retained = a.retained.canonicalize()?;
    let product = retained.join("candidate");
    let target = retained.join("target");
    require(
        !original_target.starts_with(&original) && original_target != target,
        "expected independent original Cargo target",
    )?;
    checkout(&original, ORIGINAL, false)?;
    let (host, version) = host()?;
    let receipt = |name: &str| whole().join(name).join(format!("{host}.json"));
    let source = original.join(MODEL);
    let source_sha = sha(&source)?;
    let cargo_sha = sha(&original.join("Cargo.lock"))?;
    let witness = load(&whole().join(format!("gap-range-reference/original-{host}.json")))?;
    require(
        source_sha == witness["modelSha256"] && cargo_sha == witness["cargoLockSha256"],
        "original model or Cargo lock differs from public reference",
    )?;
    let raw = run(command(Path::new("cargo"), &original)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--lib",
            "-p",
            ENGINE,
            "--target",
            &host,
            "--target-dir",
        ])
        .arg(&original_target)
        .arg("--message-format=json"))?;
    let original_engine = library(
        &manifest_native::rows(&raw)?,
        "marmish_engine",
        &original_target.join(&host).join("release"),
    )?;
    let authored = receipt("canonical-range-data");
    let recovered = receipt("canonical-recovered-range");
    let old = load(&authored)?;
    let new = load(&recovered)?;
    let lock = text(&old["workspace"]["workspaceLockSha256"])?;
    let verify_product =
        || product::public_product(&product, AUTHORED, text(&old["companion"])?, lock);
    verify_product()?;
    require(
        new["productComparison"]["changedLocks"]["workspace.lock"]["beforeSha256"] == lock,
        "wrong retained predecessor",
    )?;
    let generated_log = fs::read(retained.join("cargo-artifacts.jsonl"))?;
    require(
        bytes_sha(&generated_log) == old["cargoArtifactLogSha256"],
        "wrong retained build log",
    )?;
    let selected = artifacts::artifacts(&generated_log, &product, &target, &host)?;
    let closure = library_closure(&generated_log, &target, &host)?;
    require(
        keys(&closure)?.len() == old["cargoLibraryClosureEntries"]
            && hashed(&closure)? == old["cargoLibraryClosureSha256"],
        "wrong retained library closure",
    )?;
    let case_file = whole().join("gap-range-endpoint/cases.json");
    let cases: Vec<Value> = serde_json::from_slice(&fs::read(&case_file)?)?;
    let vectors = serde_json::to_vec(
        &cases
            .iter()
            .map(|c| json!({"id":c["id"],"wire":c["wire"],"type":"range"}))
            .collect::<Vec<_>>(),
    )?;
    let scratch = tempfile::tempdir()?;
    let reference = observe(
        &raw,
        &original_engine,
        &original_target,
        &host,
        scratch.path(),
        &vectors,
    )?;
    let generated = observe(
        &generated_log,
        Path::new(text(&selected.roles[ENGINE][1])?),
        &target,
        &host,
        scratch.path(),
        &vectors,
    )?;
    require(
        reference["results"] == generated["results"],
        "native observations differ",
    )?;
    validate(&cases, &reference["results"])?;
    checkout(&original, ORIGINAL, false)?;
    verify_product()?;
    require(
        source_sha == sha(&source)? && closure == library_closure(&generated_log, &target, &host)?,
        "source or generated libraries changed during observation",
    )?;
    let mut result = json!({"host":host,"rustc":version,"contract":"urn:marmish:engine:output-gap:range:end","casesSha256":sha(&case_file)?,"probeSha256":sha(&whole().join("gap-range-reference/probe.rs"))?,
        "observerSha256":bytes_sha(include_bytes!("native.rs")),"originalSource":ORIGINAL,"originalModelSha256":source_sha,"originalCargoLockSha256":cargo_sha,"original":reference,
        "generatedWorkspaceLockSha256":lock,"generatedBuildReceiptSha256":sha(&authored)?,"sourceEquivalenceReceiptSha256":sha(&recovered)?,"generated":generated});
    result.sort_all_objects();
    fs::write(a.out, json_bytes(&result)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_encoded_bytes_and_rejection_fields_are_required() -> Result {
        let cases = [
            json!({"id":"large","expected":"9007199254740993"}),
            json!({"id":"bad","expected":null}),
        ];
        let good = json!([{"id":"large","result":{"encoded":"9007199254740993"}},
            {"id":"bad","result":{"line":1,"column":1,"diagnostic":"invalid"}}]);
        validate(&cases, &good)?;
        let mut rounded = good.clone();
        rounded[0]["result"]["encoded"] = json!("9007199254740992");
        assert!(validate(&cases, &rounded).is_err());
        let mut missing = good.clone();
        missing[1]["result"]
            .as_object_mut()
            .unwrap()
            .remove("column");
        assert!(validate(&cases, &missing).is_err());
        assert!(
            validate(
                &[cases[0].clone(), cases[0].clone()],
                &json!([good[0], good[0]])
            )
            .is_err()
        );
        assert!(validate(&cases, &json!([good[1], good[0]])).is_err());
        Ok(())
    }
}
