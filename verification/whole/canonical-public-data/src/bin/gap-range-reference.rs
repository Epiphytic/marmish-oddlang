//! Check the JSON data contract of the native gap reason and range types.
use clap::{Parser, ValueEnum};
use public_data_qualification::{
    Result,
    gap_callers::{candidate, checked, command, reference},
    load, portable, require, sha, text, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
const UPSTREAM: &str = "af54339d250290490d62d4475a77539b7e739071";
const FRAMEWORK: &str = "3c01918dd430afcf3b57a21a5b24191f99333025";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Observe,
    Project,
}
#[derive(Parser)]
struct Args {
    #[arg(value_enum)]
    mode: Mode,
    checkout: PathBuf,
    target: PathBuf,
    output: PathBuf,
}
fn pinned(dir: &Path, expected: &str) -> Result {
    let head = checked(command("git", dir).args(["rev-parse", "HEAD"]), None)?;
    let status = checked(
        command("git", dir).args(["status", "--porcelain", "--untracked-files=all"]),
        None,
    )?;
    require(
        std::str::from_utf8(&head)?.trim() == expected && status.is_empty(),
        format!("expected clean pinned checkout at {expected}"),
    )
}
fn host(dir: &Path) -> Result<String> {
    let b = checked(command("rustc", dir).args(["+1.97.1", "-vV"]), None)?;
    let s = std::str::from_utf8(&b)?;
    require(
        s.lines().any(|s| s == format!("commit-hash: {COMPILER}")),
        "unexpected Rust compiler",
    )?;
    let rows: Vec<_> = s.lines().filter_map(|s| s.strip_prefix("host: ")).collect();
    if let [one] = rows.as_slice() {
        Ok(one.to_string())
    } else {
        Err("expected one host".into())
    }
}
fn array(v: &Value) -> Result<&Vec<Value>> {
    Ok(v.as_array().ok_or("expected array")?)
}
fn fixture() -> PathBuf {
    whole().join("gap-range-reference")
}
fn source_pin() -> Result<String> {
    sha(&whole().join("canonical-public-data/src/bin/gap-range-reference.rs"))
}
fn observe(original: &Path, target: &Path, host: &str) -> Result<Value> {
    let lane = fixture();
    let vectors = fs::read(lane.join("cases.json"))?;
    let cases: Value = serde_json::from_slice(&vectors)?;
    let (observed, codes, hashes) = reference(
        original,
        target,
        host,
        &lane.join("probe.rs"),
        &["--cfg", "with_range"],
        &[],
        &vectors,
    )?;
    let ids = |v: &Value| -> Result<Vec<String>> {
        array(v)?
            .iter()
            .map(|r| Ok(text(&r["id"])?.to_owned()))
            .collect()
    };
    let expected = ids(&cases)?;
    require(
        expected.iter().collect::<BTreeSet<_>>().len() == expected.len()
            && ids(&observed)? == expected,
        "incomplete or ambiguous case results",
    )?;
    pinned(original, UPSTREAM)?;
    Ok(
        json!({"original":UPSTREAM,"host":host,"compiler":COMPILER,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,"modelSha256":sha(&original.join("crates/marmish-engine/src/model.rs"))?,"casesSha256":sha(&lane.join("cases.json"))?,"probeSha256":sha(&lane.join("probe.rs"))?,"observerSha256":source_pin()?,"copyControl":codes,"librarySha256":hashes,"results":observed}),
    )
}
fn project(framework: &Path, target: &Path, host: &str) -> Result<Value> {
    let lane = fixture();
    let data = portable();
    let spec_file = data.join("engine-gap-reason-package.json");
    let reference = data.join("engine-gap-reason-v1/package.lock");
    let archive = lane.join(format!("original-{host}.json"));
    let original = load(&archive)?;
    require(
        original["casesSha256"] == sha(&lane.join("cases.json"))?
            && original["probeSha256"] == sha(&lane.join("probe.rs"))?,
        "reference wire probe changed",
    )?;
    let all = load(&lane.join("cases.json"))?;
    let cases: Vec<_> = array(&all)?
        .iter()
        .filter(|v| v["type"] == "reason")
        .collect();
    let ids = cases
        .iter()
        .map(|v| text(&v["id"]))
        .collect::<Result<BTreeSet<_>>>()?;
    let expected: Vec<_> = array(&original["results"])?
        .iter()
        .filter(|v| v["id"].as_str().is_some_and(|id| ids.contains(id)))
        .collect();
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    checked(
        command("cargo", framework)
            .args([
                "+1.96.1",
                "build",
                "--locked",
                "--offline",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(target),
        None,
    )?;
    let compiler = work.join("oddlang");
    fs::copy(target.join("debug/oddlang"), &compiler)?;
    let compiler_sha = sha(&compiler)?;
    let spec = load(&spec_file)?;
    let staged = work.join("inputs");
    fs::create_dir(&staged)?;
    let mut pins = vec![&spec["cargoLock"]];
    for component in array(&spec["components"])? {
        pins.extend([&component["graph"], &component["overlay"]]);
    }
    for pin in pins {
        let rel = Path::new(text(&pin["path"])?);
        require(
            rel.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "unsafe input path",
        )?;
        let src = data.join(rel);
        let dest = staged.join(rel);
        require(
            sha(&src)? == pin["sha256"],
            "component graph, overlay or Cargo lock changed",
        )?;
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(src, dest)?;
    }
    let manifest = staged.join(spec_file.file_name().unwrap());
    fs::copy(&spec_file, &manifest)?;
    let product = work.join("product");
    checked(
        command(compiler.to_str().ok_or("UTF-8 compiler path")?, work)
            .args(["build", "native-package", "rust"])
            .arg(manifest)
            .arg(&product),
        None,
    )?;
    require(
        fs::read(product.join("package.lock"))? == fs::read(&reference)?,
        "generated package lock changed",
    )?;
    for (path, hash) in load(&reference)?["generatedFiles"]
        .as_object()
        .ok_or("generated file map")?
    {
        require(
            sha(&product.join(path))? == *hash,
            "generated files differ from seal",
        )?;
    }
    let (observations, codes) = candidate(
        work,
        host,
        &lane.join("probe.rs"),
        &[],
        &serde_json::to_vec(&cases)?,
    )?;
    require(
        observations == json!(expected) && expected.len() == cases.len(),
        "candidate wire outcomes differ",
    )?;
    require(
        original["copyControl"] == codes,
        "reference Copy diagnostic differs",
    )?;
    pinned(framework, FRAMEWORK)?;
    Ok(
        json!({"host":host,"rustcCommit":COMPILER,"framework":FRAMEWORK,"frameworkBinarySha256":compiler_sha,"callerLockSha256":sha(&lane.join("caller.lock"))?,"callerManifestSha256":sha(&lane.join("caller.toml"))?,"manifestSha256":sha(&spec_file)?,"packageLockSha256":sha(&reference)?,"referenceSha256":sha(&archive)?,"verifierSha256":source_pin()?,"copyControl":codes,"observations":observations}),
    )
}
fn main() -> Result {
    let args = Args::parse();
    let checkout = args.checkout.canonicalize()?;
    let target = std::path::absolute(&args.target)?;
    pinned(
        &checkout,
        match args.mode {
            Mode::Observe => UPSTREAM,
            Mode::Project => FRAMEWORK,
        },
    )?;
    require(
        !target.starts_with(&checkout),
        "use an external Cargo target directory",
    )?;
    fs::create_dir_all(&target)?;
    let target = target.canonicalize()?;
    require(
        !target.starts_with(&checkout),
        "use an external Cargo target directory",
    )?;
    let host = host(&checkout)?;
    let mut doc = match args.mode {
        Mode::Observe => observe(&checkout, &target, &host)?,
        Mode::Project => project(&checkout, &target, &host)?,
    };
    doc.sort_all_objects();
    fs::write(args.output, public_data_qualification::json_bytes(&doc)?)?;
    Ok(())
}
