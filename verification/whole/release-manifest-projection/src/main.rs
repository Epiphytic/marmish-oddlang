//! Compare the sealed baseline and changed parser with the independent native observations.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox},
    json_bytes, list, load, portable, require, sha, stdout, text, whole,
};
use release_manifest_projection::{cargo, compare_records, run, rustdoc};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PLAN: &str = "native-release-manifest-package.json";
const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
const GENERATED: &str = "src/group_1/__projection_1.rs";
fn here() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
#[derive(Parser)]
struct Args {
    framework: PathBuf,
    original: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    compiler_target: PathBuf,
    #[arg(long)]
    original_target: PathBuf,
    #[arg(long)]
    framework_revision: String,
}
fn write(path: &Path, v: &Value) -> Result {
    Ok(fs::write(path, json_bytes(v)?)?)
}
fn stage(dir: &Path, changed: Option<&Value>) -> Result<PathBuf> {
    fs::create_dir(dir)?;
    let portable = portable();
    let mut manifest = load(&portable.join(PLAN))?;
    assembly::pinned_copy(&portable, &manifest["cargoLock"], dir)?;
    let components = manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    for c in components.iter() {
        for k in ["graph", "overlay"] {
            assembly::pinned_copy(&portable, &c[k], dir)?;
        }
    }
    if let Some(changed) = changed {
        let mut components = components
            .iter_mut()
            .filter(|c| c["kind"] == "line-grammar");
        let line = components.next().ok_or("missing line grammar")?;
        require(components.next().is_none(), "ambiguous line grammar")?;
        let graph = dir.join(text(&line["graph"]["path"])?);
        write(&graph, changed)?;
        line["graph"]["sha256"] = json!(sha(&graph)?);
    }
    let plan = dir.join(PLAN);
    write(&plan, &manifest)?;
    Ok(plan)
}
fn changed_graph(mut graph: Value) -> Result<Value> {
    let mut matched = 0;
    for row in graph["rows"].as_array_mut().ok_or("missing rows")? {
        for term in row.as_array_mut().ok_or("invalid row")? {
            if term["capture"] == "file-4-size" {
                require(
                    term["maximum"] == 16 * 1024 * 1024,
                    "unexpected license cap",
                )?;
                term["maximum"] = json!(16 * 1024 * 1024 + 1);
                matched += 1;
            }
        }
    }
    require(matched == 1, "expected exactly one license cap")?;
    Ok(graph)
}
fn check_observations(observations: &Path, mode: &str, path: &Path, target: &Path) -> Result {
    run(cargo()
        .args(["run", "--quiet", "--locked", "--offline", "--manifest-path"])
        .arg(observations.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target)
        .args(["--", mode])
        .arg(path))?;
    Ok(())
}
fn copy_dir(src: &Path, dst: &Path) -> Result {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let dest = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?
        } else {
            fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}
fn probe(product: &Path, scratch: &Path, inputs: &Path, observer: &Path) -> Result<PathBuf> {
    let test = scratch.join("test");
    copy_dir(&product.join("src"), &test.join("src"))?;
    let mut manifest = fs::read(product.join("Cargo.toml"))?;
    manifest
        .extend_from_slice(b"\n[dev-dependencies]\nserde_json = \"=1.0.151\"\nhex = \"=0.4.3\"\n");
    fs::write(test.join("Cargo.toml"), manifest)?;
    fs::copy(here().join("observer.Cargo.lock"), test.join("Cargo.lock"))?;
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(test.join("src/lib.rs"))?
        .write_all(b"\n#[cfg(test)]\n#[path = \"oracle.rs\"]\nmod oracle;\n")?;
    fs::copy(observer, test.join("src/oracle.rs"))?;
    let output = scratch.join("results.json");
    let case = "oracle::manifest_contract_probe";
    let stdout = run(cargo()
        .current_dir(test)
        .env("MANIFEST_CASES", inputs)
        .env("MANIFEST_RESULT", &output)
        .args(["test", "--release", "--locked", "--offline", "--target-dir"])
        .arg(scratch.parent().unwrap().join("caller-target"))
        .args(["--lib", "--", "--exact", case]))?;
    require(
        std::str::from_utf8(&stdout)?
            .lines()
            .filter(|l| *l == format!("test {case} ... ok"))
            .count()
            == 1,
        "native probe did not run",
    )?;
    Ok(output)
}
fn writable(path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(path)?;
    Ok(path.canonicalize()?)
}
fn main() -> Result {
    let args = Args::parse();
    let root = assembly::root().canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let compiler_target = writable(&args.compiler_target)?;
    let original_target = writable(&args.original_target)?;
    let rev = stdout(
        Command::new("git")
            .current_dir(&root)
            .args(["rev-parse", "HEAD"]),
    )?
    .trim()
    .to_owned();
    for (path, revision) in [
        (&root, &*rev),
        (&original, ORIGINAL),
        (&framework, &*args.framework_revision),
    ] {
        assembly::checkout(path, revision, false)?;
    }
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rustc
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing Rust host")?;
    require(
        matches!(host, assembly::DARWIN | assembly::LINUX),
        "no native witness for host",
    )?;
    let obs = whole().join("release-manifest");
    let oracle_path = obs.join(format!("{host}.json"));
    let oracle = load(&oracle_path)?;
    let vectors_path = obs.join("vectors.json");
    let caller = obs.join("probe.rs");
    let source = original.join(SOURCE);
    require(
        oracle["originalRevision"] == ORIGINAL && oracle["sourcePins"][SOURCE] == sha(&source)?,
        "release parser source provenance changed",
    )?;
    let raw = fs::read_to_string(&source)?;
    let old = "                    16 * 1024 * 1024\n";
    require(
        raw.matches(old).count() == 1,
        "expected one observed changed-source expression",
    )?;
    let new = raw.replace(old, "                    16 * 1024 * 1024 + 1\n");
    require(
        assembly::bytes_sha(new.as_bytes()) == oracle["mutatedSourceSha256"],
        "changed-source digest mismatch",
    )?;
    let graph = load(&portable().join("release-manifest-v1/graph.json"))?;
    let overlay = load(&portable().join("release-manifest-v1/rust.json"))?;
    require(
        graph["textGrammars"]
            == json!([load(
                &portable().join("release-version-grammar-v1/graph.json")
            )?]),
        "linked release version grammar changed",
    )?;
    let changed = changed_graph(graph.clone())?;
    let [delta] = list(&oracle["mutation"])?.as_slice() else {
        return Err("expected one changed oracle row".into());
    };
    require(
        delta["case"] == "size-3-16777217",
        "unexpected changed native case",
    )?;
    let build = tempfile::Builder::new()
        .prefix("odd-manifest-inputs-")
        .tempdir()?;
    let private = tempfile::Builder::new()
        .prefix("odd-manifest-oracles-")
        .tempdir()?;
    let work = build.path().canonicalize()?;
    let scratch = private.path().canonicalize()?;
    let conformance_target = scratch.join("conformance-target");
    check_observations(&obs, "check-receipt", &oracle_path, &conformance_target)?;
    run(cargo()
        .current_dir(&framework)
        .args([
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    let compiler = work.join("oddlang");
    fs::copy(compiler_target.join("debug/oddlang"), &compiler)?;
    let cache = work.join("empty-provider-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[
            &root,
            &framework,
            &original,
            &original_target,
            &compiler_target,
            &scratch,
        ],
        &cache,
    )?;
    let denied = BTreeMap::from([
        ("original-parser", source.clone()),
        ("oracle-caller", caller.clone()),
        ("original-observations", oracle_path.clone()),
        ("blind-vectors", vectors_path.clone()),
        ("compiler-source", framework.join("src/lib.rs")),
    ]);
    sandbox.deny(&denied.values().cloned().collect::<Vec<_>>())?;
    let mut products = BTreeMap::new();
    for (tag, change) in [("baseline", None), ("changed", Some(&changed))] {
        let plan = stage(&work.join(tag), change)?;
        let product = work.join(format!("{tag}-product"));
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&product))?;
        run(sandbox.command(&product)?.args([
            "cargo",
            "build",
            "--release",
            "--locked",
            "--offline",
        ]))?;
        products.insert(tag, product);
    }
    require(
        fs::read(products["baseline"].join("package.lock"))?
            == fs::read(portable().join("release-manifest-v1/package.lock"))?,
        "baseline package.lock differs",
    )?;
    let original_doc = rustdoc(&original, &original_target)?;
    let generated_doc = rustdoc(&products["baseline"], &scratch.join("rustdoc-generated"))?;
    let shape = compare_records(&original_doc, &generated_doc, GENERATED, &graph, &overlay)?;
    let vectors = load(&vectors_path)?;
    let inputs: Vec<_> = list(&vectors)?
        .iter()
        .map(|v| json!({"case":v["case"],"hex":v["hex"]}))
        .collect();
    let input = scratch.join("inputs.json");
    write(&input, &json!(inputs))?;
    let mut lanes = json!({});
    for (tag, product) in products {
        let dir = scratch.join(tag);
        fs::create_dir(&dir)?;
        let results = probe(&product, &dir, &input, &caller)?;
        check_observations(
            &obs,
            if tag == "changed" {
                "check-changed"
            } else {
                "check"
            },
            &results,
            &conformance_target,
        )?;
        let rows = load(&results)?;
        let mut expected = oracle["baseline"].clone();
        if tag == "changed" {
            let row = expected
                .as_array_mut()
                .ok_or("missing baseline")?
                .iter_mut()
                .find(|r| r["case"] == delta["case"])
                .ok_or("missing changed case")?;
            row["result"] = delta["result"].clone();
        }
        require(rows == expected, format!("native rows disagree in {tag}"))?;
        lanes[tag] = json!({"cases":list(&rows)?.len(),"accepted":list(&rows)?.iter().filter(|r|r["result"].get("error").is_none()).count(),"resultsSha256":sha(&results)?,"packageLockSha256":sha(&product.join("package.lock"))?,"projectionSha256":sha(&product.join(GENERATED))?});
    }
    write(
        &args.receipt,
        &json!({
            "format":"marmish-pure-release-manifest-projection-v1","original":ORIGINAL,"framework":args.framework_revision,"companion":rev,"platform":host,
            "rustc":rustc.lines().collect::<Vec<_>>(),"compilerSha256":sha(&compiler)?,"sourceSha256":sha(&source)?,"changedSourceSha256":oracle["mutatedSourceSha256"],
            "oracleReceiptSha256":sha(&oracle_path)?,"vectorsSha256":sha(&vectors_path)?,"callerSha256":sha(&caller)?,"observerCargoLockSha256":sha(&here().join("observer.Cargo.lock"))?,
            "deniedSourceProbes":denied.keys().collect::<Vec<_>>(),"resultShapes":{"fieldsAndTraits":shape,"originalRustdocSha256":sha(&original_doc)?,"generatedRustdocSha256":sha(&generated_doc)?},
            "changedCases":[delta["case"]],"lanes":lanes
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutant_requires_one_original_bound() -> Result {
        let row = json!({"capture":"file-4-size","maximum":16*1024*1024});
        let changed = changed_graph(json!({"rows":[[row]]}))?;
        assert_eq!(changed["rows"][0][0]["maximum"], 16 * 1024 * 1024 + 1);
        assert!(changed_graph(changed).is_err());
        assert!(changed_graph(json!({"rows":[[row,row]]})).is_err());
        assert!(changed_graph(json!({"rows":[[]]})).is_err());
        Ok(())
    }
}
