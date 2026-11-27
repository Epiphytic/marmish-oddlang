//! Recover the pinned field conjunction and observe its single-operand source mutation.
use copy_checkpoint_replay::{build_compiler, stage};
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox},
    json_bytes, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const PIN: &str = "4258cf67bd9153de7da86a6f4e1b03a9fbb66f35";
const CRATE: &str = "crates/marmish-stream";
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";
const RELATION: &str = "urn:marmish:copy:launch:matches-manifest";
const METHOD: &str = "crate::copy_protocol::Launch::matches_manifest";
const PACKAGE: &str = "stream-copy-source-correspondence-package.json";

pub fn archive(repo: &Path, revision: &str, dest: &Path) -> Result {
    let tar = tempfile::NamedTempFile::new()?;
    run(Command::new("git")
        .current_dir(repo)
        .args(["archive", "--output"])
        .arg(tar.path())
        .arg(revision))?;
    fs::create_dir(dest)?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(tar.path())
        .arg("-C")
        .arg(dest))?;
    Ok(())
}
fn recover(
    framework: &Path,
    cli: &Path,
    checkout: &Path,
    host: &str,
    output: &Path,
) -> Result<PathBuf> {
    fs::create_dir(output)?;
    let source = checkout.join(SOURCE);
    let crate_dir = checkout.join(CRATE);
    let evidence = output.join("evidence.json");
    let selection = output.join("selection.json");
    let graph = output.join("graph.json");
    // The historical compiler release supplies this exact rustc_private capture driver.
    let observations = run(Command::new("bash")
        .arg(framework.join("verification/rust-recovery/run.sh"))
        .arg(crate_dir.join("Cargo.toml"))
        .args(["marmish-stream", "lib", "marmish_stream", host, "default"])
        .env("ODD_RECOVERY_FIELD_CONJUNCTION", METHOD))?;
    fs::write(&evidence, observations)?;
    fs::write(
        &selection,
        json_bytes(
            &json!({"format":"oddlang-field-conjunction-selection-v1","relation":RELATION,"sourceSha256":sha(&source)?,"evidenceSha256":sha(&evidence)?}),
        )?,
    )?;
    run(Command::new(cli)
        .args(["recover", "field-conjunction"])
        .arg(crate_dir)
        .arg(source)
        .arg(evidence)
        .arg(selection)
        .arg(portable().join("copy-source-correspondence-v1/template.json"))
        .arg(portable().join("copy-field-relations-v1/rust.json"))
        .arg(&graph))?;
    Ok(graph)
}
fn verify_graphs(base: &Path, mutant: &Path, data: &Path) -> Result<usize> {
    let mut released = load(&portable().join("copy-field-relations-v1/graph.json"))?;
    for (group, optional) in [("relations", "failure"), ("calls", "output")] {
        for row in released[group]
            .as_object_mut()
            .ok_or("invalid released graph")?
            .values_mut()
        {
            row.as_object_mut()
                .ok_or("invalid graph row")?
                .entry(optional)
                .or_insert(Value::Null);
        }
    }
    require(
        load(base)? == released,
        "recovered field conjunction differs from released relation",
    )?;
    for file in ["graph.json", "evidence.json", "selection.json"] {
        require(
            fs::read(base.with_file_name(file))? == fs::read(data.join(file))?,
            format!("recovered {file} differs from archive"),
        )?;
    }
    let steps = released["relations"][RELATION]["steps"]
        .as_array_mut()
        .ok_or("missing relation steps")?;
    let count = steps.len();
    let field = &mut steps.get_mut(1).ok_or("missing second relation step")?["right"]["field"];
    *field = json!(format!(
        "{}:server",
        text(field)?
            .strip_suffix(":transfer")
            .ok_or("missing original transfer comparison")?
    ));
    require(
        load(mutant)? == released,
        "source mutation changed unexpected graph edges",
    )?;
    Ok(count)
}
fn check_inputs(dir: &Path) -> Result {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            check_inputs(&path)?
        } else {
            require(
                path.extension().is_none_or(|e| e != "rs") && entry.file_name() != "evidence.json",
                "recovery source or evidence entered projection inputs",
            )?;
        }
    }
    Ok(())
}
fn relation_hash(lock: &Path) -> Result<String> {
    let value = load(lock)?;
    let component = value["components"]
        .as_array()
        .and_then(|c| c.last())
        .ok_or("missing relation component")?;
    require(
        component["kind"] == "field-relation",
        "unexpected final component",
    )?;
    Ok(text(&component["sourceSha256"])?.into())
}
pub fn verify(original: &Path, framework: &Path, receipt: &Path) -> Result {
    let original = original.canonicalize()?;
    let framework = framework.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    require(
        stdout(
            Command::new("git")
                .current_dir(&framework)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == PIN,
        "unexpected compiler revision",
    )?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    let private = tempfile::tempdir()?;
    let scratch = tempfile::tempdir()?;
    let work = scratch.path().canonicalize()?;
    let sealed = private.path().join("framework");
    archive(&framework, PIN, &sealed)?;
    let built = build_compiler(&sealed, &private.path().join("tools"), "+1.97.1")?;
    let cli = work.join("oddlang");
    fs::copy(&built, &cli)?;
    let mutant = private.path().join("mutant");
    archive(&original, ORIGINAL, &mutant)?;
    let src = mutant.join(SOURCE);
    let old = fs::read_to_string(&src)?;
    let operand = "&& self.transfer == manifest.transfer";
    require(
        old.matches(operand).count() == 1,
        "ambiguous or absent source comparison",
    )?;
    fs::write(
        src,
        old.replace(operand, "&& self.transfer == manifest.server"),
    )?;
    let base_graph = recover(
        &sealed,
        &cli,
        &original,
        host,
        &private.path().join("base-recovery"),
    )?;
    let mutant_graph = recover(
        &sealed,
        &cli,
        &mutant,
        host,
        &private.path().join("mutant-recovery"),
    )?;
    let data = portable().join("copy-source-correspondence-v1");
    let comparisons = verify_graphs(&base_graph, &mutant_graph, &data)?;
    let inputs = work.join("inputs");
    let (plan, mut manifest) = stage(&inputs, PACKAGE, false)?;
    let caller = whole().join("copy-field-relations/caller.rs");
    let cache = work.join("empty-provider-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[assembly::root(), &original, &framework, private.path()],
        &cache,
    )?;
    let project = |name: &str| -> Result<PathBuf> {
        check_inputs(&inputs)?;
        sandbox.deny(&[
            original.join(SOURCE),
            mutant.join(SOURCE),
            caller.clone(),
            data.join("evidence.json"),
        ])?;
        let dest = work.join(name);
        run(sandbox
            .command(&work)?
            .arg(&cli)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&dest))?;
        run(sandbox.command(&dest)?.args([
            "cargo",
            "build",
            "--locked",
            "--offline",
            "--release",
        ]))?;
        Ok(dest)
    };
    let native = project("base-package")?;
    let graph = &mut manifest["components"]
        .as_array_mut()
        .and_then(|c| c.last_mut())
        .ok_or("missing component")?["graph"];
    fs::copy(&mutant_graph, inputs.join(text(&graph["path"])?))?;
    graph["sha256"] = json!(sha(&mutant_graph)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let altered = project("altered-package")?;
    let comparison =
        super::compare_correspondence(&original, &native, &mutant, &altered, private.path())?;
    let base_lock = native.join("package.lock");
    let altered_lock = altered.join("package.lock");
    require(
        fs::read(&base_lock)? == fs::read(data.join("package.lock"))?,
        "original package lock differs from archive",
    )?;
    let base_code = relation_hash(&base_lock)?;
    let mutant_code = relation_hash(&altered_lock)?;
    require(
        base_code != mutant_code,
        "mutation produced the original source",
    )?;
    let value = json!({"original":ORIGINAL,"framework":PIN,"rust":rust.trim(),"comparisons":comparisons,"casesPerImplementation":comparison["casesPerImplementation"],"changedCases":comparison["changedCases"],"readDenied":true,"callerSha256":sha(&caller)?,"originalGraphSha256":sha(&base_graph)?,"mutatedGraphSha256":sha(&mutant_graph)?,"originalEvidenceSha256":sha(&base_graph.with_file_name("evidence.json"))?,"mutatedEvidenceSha256":sha(&mutant_graph.with_file_name("evidence.json"))?,"originalGeneratedRelationSha256":base_code,"mutantGeneratedRelationSha256":mutant_code,"originalPackageLockSha256":sha(&base_lock)?,"mutantPackageLockSha256":sha(&altered_lock)?});
    fs::write(receipt, json_bytes(&value)?)?;
    println!("Qualified {comparisons} source comparisons and four native implementations.");
    Ok(())
}
