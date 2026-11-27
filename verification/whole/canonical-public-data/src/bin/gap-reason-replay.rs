//! Qualify recovered MissingReason with native downstream callers and an independent wire oracle.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{COMPILER, DARWIN, LINUX, ORIGINAL, checkout, root},
    gap_callers::{candidate, checked, command, reference},
    gap_reason::recover,
    json_bytes, list, load, portable, require, sha, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const MODEL: &str = "crates/marmish-engine/src/model.rs";
#[derive(Parser)]
struct Args {
    framework: PathBuf,
    original: PathBuf,
    output: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    compiler_target: PathBuf,
    #[arg(long)]
    reference_target: PathBuf,
    #[arg(long)]
    mutation_target: PathBuf,
}
fn mutated_source(original: &Path, work: &Path, bundle: &Path) -> Result<(PathBuf, PathBuf)> {
    let mutant = work.join("mutated-application");
    fs::create_dir(&mutant)?;
    let archive = work.join("original.tar");
    checked(
        command("git", original)
            .args(["archive", ORIGINAL, "-o"])
            .arg(&archive),
        None,
    )?;
    checked(command("tar", &mutant).arg("-xf").arg(&archive), None)?;
    fs::remove_file(archive)?;
    let source = mutant.join(MODEL);
    let raw = fs::read_to_string(&source)?;
    let before = "pub enum MissingReason {\n    Limit,\n    StorageFailure,\n";
    require(
        raw.matches(before).count() == 1,
        "ambiguous pinned reason declaration",
    )?;
    let after = "pub enum MissingReason {\n    #[serde(rename = \"Storage-Failure\")]\n    StorageFailure,\n    Limit,\n";
    fs::write(&source, raw.replacen(before, after, 1))?;
    let mut selection = load(&bundle.join("selection.json"))?;
    selection["sourceSha256"] = json!(sha(&source)?);
    let path = work.join("changed-selection.json");
    fs::write(&path, json_bytes(&selection)?)?;
    Ok((mutant, path))
}
fn check_contracts(work: &Path, bundle: &Path, manual: &Path) -> Result {
    let base = work.join("baseline/recovered");
    let changed = work.join("mutation/recovered");
    for name in ["graph.json", "rust.json", "recovery.lock"] {
        require(
            fs::read(base.join(name))? == fs::read(bundle.join(name))?,
            format!("baseline recovery drift: {name}"),
        )?;
    }
    let mut graph = load(&base.join("graph.json"))?;
    let mut overlay = load(&base.join("rust.json"))?;
    require(
        graph == load(&manual.join("graph.json"))? && overlay == load(&manual.join("rust.json"))?,
        "independent baseline differs",
    )?;
    let choices = graph["stringEnums"]
        .as_array_mut()
        .ok_or("missing string enum")?;
    let [choice] = choices.as_mut_slice() else {
        return Err("expected one string enum".into());
    };
    let id = text(&choice["id"])?.to_owned();
    let variants = choice["variants"]
        .as_array_mut()
        .ok_or("missing variants")?;
    require(variants.len() >= 2, "expected two variants")?;
    variants[1]["wire"] = json!("Storage-Failure");
    variants.swap(0, 1);
    let order = overlay["variantOrder"][id]
        .as_array_mut()
        .ok_or("missing variant order")?;
    require(order.len() >= 2, "missing variant pair")?;
    order.swap(0, 1);
    require(
        graph == load(&changed.join("graph.json"))? && overlay == load(&changed.join("rust.json"))?,
        "unexpected source mutation",
    )
}
fn observe(
    native: &Path,
    target: &Path,
    host: &str,
    probe: &Path,
    work: &Path,
    lane: &str,
    cases: &[Value],
) -> Result<Value> {
    let vectors = serde_json::to_vec(cases)?;
    let (expected, codes, hashes) = {
        let gate = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(target.join(".oddlang-missing-reason-reference.lock"))?;
        gate.lock()?;
        reference(native, target, host, probe, &[], &[lane], &vectors)?
    };
    let (actual, generated_codes) = candidate(work, host, probe, &[lane], &vectors)?;
    require(
        actual == expected && generated_codes == codes,
        "native and generated callers differ",
    )?;
    Ok(json!({"originalLibrarySha256":hashes,"output":actual,"copyControl":codes}))
}
fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    let framework = a.framework.canonicalize()?;
    let companion = String::from_utf8(checked(
        command("git", root()).args(["rev-parse", "HEAD"]),
        None,
    )?)?
    .trim()
    .to_owned();
    checkout(root(), &companion, false)?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, &a.framework_revision, false)?;
    let version = String::from_utf8(checked(
        command("rustc", &framework).args(["+1.97.1", "-vV"]),
        None,
    )?)?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing Rust host")?;
    require(
        [DARWIN, LINUX].contains(&host)
            && version
                .lines()
                .any(|s| s == format!("commit-hash: {COMPILER}")),
        "unexpected compiler or host",
    )?;
    let target = std::path::absolute(a.compiler_target)?;
    for t in [&a.reference_target, &a.mutation_target] {
        fs::create_dir_all(t)?;
    }
    let native_target = a.reference_target.canonicalize()?;
    let mutant_target = a.mutation_target.canonicalize()?;
    require(
        native_target != mutant_target,
        "use separate native target directories",
    )?;
    checked(
        command("cargo", &framework)
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(&target),
        None,
    )?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let compiler = work.join("oddlang");
    fs::copy(target.join("debug/oddlang"), &compiler)?;
    let bundle = portable().join("engine-gap-reason-recovered-v1");
    let manual = portable().join("engine-gap-reason-v1");
    let reference_dir = whole().join("gap-range-reference");
    let reference_path = reference_dir.join(format!("original-{host}.json"));
    let archive = load(&reference_path)?;
    let case_path = reference_dir.join("cases.json");
    let source = original.join(MODEL);
    let source_hash = sha(&source)?;
    require(
        archive["modelSha256"] == source_hash && archive["casesSha256"] == sha(&case_path)?,
        "wrong reference evidence",
    )?;
    let catalog: Vec<Value> = list(&load(&case_path)?)?
        .iter()
        .filter(|c| c["type"] == "reason")
        .cloned()
        .collect();
    let expected: Vec<&Value> = list(&archive["results"])?
        .iter()
        .filter(|r| catalog.iter().any(|c| c["id"] == r["id"]))
        .collect();
    require(expected.len() == 17, "incomplete baseline cases")?;
    let baseline = recover(
        &compiler,
        &source,
        &bundle.join("selection.json"),
        &work.join("baseline"),
    )?;
    let (mutant, selection) = mutated_source(&original, &work, &bundle)?;
    let mutation = recover(
        &compiler,
        &mutant.join(MODEL),
        &selection,
        &work.join("mutation"),
    )?;
    check_contracts(&work, &bundle, &manual)?;
    let probe = whole().join("gap-reason-recovery/probe.rs");
    let mut base = observe(
        &original,
        &native_target,
        host,
        &probe,
        &work.join("baseline"),
        "baseline",
        &catalog,
    )?;
    require(
        base["output"]["observations"] == json!(expected),
        "baseline differs from frozen witnesses",
    )?;
    let sealed = load(&manual.join("package.lock"))?;
    let emitted = &baseline["candidate"];
    require(
        emitted["generatedFiles"] == sealed["generatedFiles"]
            && emitted["projection"] == sealed["components"][0]["projection"],
        "projection differs from independent package",
    )?;
    base.as_object_mut()
        .ok_or("invalid baseline observations")?
        .shift_insert(0, "candidate".into(), emitted.clone());
    let mut cases = catalog.clone();
    cases.push(json!({"id":"new-storage-wire-name","type":"reason","wire":"\"Storage-Failure\""}));
    let mut change = observe(
        &mutant,
        &mutant_target,
        host,
        &probe,
        &work.join("mutation"),
        "mutation",
        &cases,
    )?;
    let rows = list(&change["output"]["observations"])?;
    let old: Vec<_> = catalog
        .iter()
        .filter(|c| c["wire"] == "\"StorageFailure\"")
        .collect();
    require(
        !old.is_empty()
            && old.iter().all(|c| {
                rows.iter()
                    .any(|r| r["id"] == c["id"] && r["result"].get("diagnostic").is_some())
            })
            && rows.iter().any(|r| {
                r["id"] == "new-storage-wire-name"
                    && r["result"] == json!({"encoded":"\"Storage-Failure\""})
            }),
        "mutated wire admission did not change",
    )?;
    change
        .as_object_mut()
        .ok_or("invalid mutation observations")?
        .shift_insert(0, "candidate".into(), mutation["candidate"].clone());
    checkout(&original, ORIGINAL, false)?;
    let receipt = json!({"format":"marmish-gap-reason-recovery-v1","host":host,"original":ORIGINAL,"framework":a.framework_revision,"companion":companion,"rustc":COMPILER,"compilerSha256":sha(&compiler)?,"sourceSha256":source_hash,"probeSha256":sha(&probe)?,"referenceSha256":sha(&reference_path)?,"baselineRecovery":baseline["recovery"],"mutationRecovery":mutation["recovery"],"lanes":{"baseline":base,"mutation":change}});
    let output = std::path::absolute(a.output)?;
    let mut file =
        tempfile::NamedTempFile::new_in(output.parent().ok_or("missing receipt parent")?)?;
    file.write_all(&json_bytes(&receipt)?)?;
    file.persist(output)?;
    Ok(())
}
