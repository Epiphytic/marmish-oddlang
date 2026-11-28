//! Qualify the role-bearing compact package against previously observed typed and wire callers.
use super::{MODEL, derived, qualify, sealed, write_json};
use public_data_qualification::{
    Result,
    assembly::{self, COMPILER, DARWIN, Sandbox},
    capture, endpoint_contract, list, load, portable,
    product::pinned,
    require, run, sha, whole,
};
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::Command};

pub(super) const FRAMEWORK: &str = "f74fea91c697e1e59ba894fb5284cb1717efec0f";
const ROLE: &str = "73e9dd4349d990e8dfd7160473799f6ec5b027a7";
const COMPACT: &str = "a18c5f83bd1fcf3869fbcde13ff43bd6bed56d76";
const MANIFEST: &str = "engine-gap-range-derived-source-api-package.json";
const BINDING: &str = "engine-gap-range-native-idioms-v1/rust.json";
const END: &str = "urn:marmish:engine:output-gap:range:end";

fn stage(inputs: &Path, witness: &Value) -> Result {
    derived::stage(inputs)?;
    let compact = inputs.join(derived::MANIFEST);
    pinned(&portable().join(derived::MANIFEST), COMPACT)?;
    let source = portable().join(MANIFEST);
    let mut spec = load(&source)?;
    let pin = spec["components"][1]
        .as_object_mut()
        .and_then(|o| o.remove("sourceApi"))
        .ok_or("missing source API pin")?;
    require(
        spec == load(&compact)? && pin["path"] == BINDING,
        "role manifest changed other compact inputs",
    )?;
    fs::remove_file(compact)?;
    let binding = assembly::pinned_copy(&portable(), &pin, inputs)?;
    pinned(&binding, ROLE)?;
    require(
        pin["sha256"] == witness["bindingSha256"],
        "source API binding differs from endpoint qualification",
    )?;
    fs::copy(source, inputs.join(MANIFEST))?;
    Ok(())
}

fn check_lock(path: &Path, manifest: &Path, baseline: &Path) -> Result {
    let mut lock = load(path)?;
    let earlier = load(baseline)?;
    let spec = load(manifest)?;
    let pin = &spec["components"][1]["sourceApi"]["sha256"];
    require(
        lock["manifestSha256"] == sha(manifest)?,
        "package lock omits manifest pin",
    )?;
    for pointer in ["/components/1", "/components/1/projection"] {
        let digest = lock
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .and_then(|o| o.remove("sourceApiSha256"));
        require(
            digest.as_ref() == Some(pin),
            "package lock omits role binding",
        )?;
    }
    lock["manifestSha256"] = earlier["manifestSha256"].clone();
    require(
        lock == earlier,
        "compact lock changed beyond role provenance",
    )
}

fn controls(
    binding: &Path,
    manifest: &Path,
    work: &Path,
    build: impl Fn(&Path) -> Result<Command>,
) -> Result<Value> {
    let original = load(binding)?;
    let mut changed = original.clone();
    changed["flattenModule"] = json!(
        !original["flattenModule"]
            .as_bool()
            .ok_or("missing flattenModule")?
    );
    write_json(binding, changed)?;
    let reject = |name: &str, messages: &[&str]| -> Result<Value> {
        let output = work.join(name);
        let result = capture(&mut build(&output)?, 600, None)?;
        let message = String::from_utf8(result.stderr)?;
        require(
            !result.status.success()
                && output.symlink_metadata().is_err()
                && messages.iter().all(|s| message.contains(s)),
            format!("unexpected {name} outcome: {message}"),
        )?;
        Ok(json!(message.trim()))
    };
    let mut diagnostics = json!({});
    diagnostics["stale-pin"] = reject("stale-pin", &["input digest mismatch", BINDING])?;
    let mut spec = load(manifest)?;
    spec["components"][1]["sourceApi"]["sha256"] = json!(sha(binding)?);
    write_json(manifest, spec.clone())?;
    diagnostics["changed-binding"] = reject(
        "changed-binding",
        &["source API overlay differs from recovered binding"],
    )?;
    let mut changed = original;
    changed["sourceApi"]["fields"][END]["visibility"] = json!("private");
    write_json(binding, changed)?;
    spec["components"][1]["sourceApi"]["sha256"] = json!(sha(binding)?);
    write_json(manifest, spec)?;
    diagnostics["private-field"] =
        reject("private-field", &["unknown variant", "private", "public"])?;
    Ok(diagnostics)
}

pub(super) fn verify(
    work: &Path,
    args: &derived::Args,
    host: &str,
    builder: &Value,
    companion: &str,
) -> Result<Value> {
    require(host == DARWIN, "expected native Darwin ARM64")?;
    let version = qualify::version("+1.97.1", COMPILER, host)?;
    let cases = endpoint_contract::validate()?;
    let endpoint = whole().join("gap-range-endpoint");
    let witness_path = endpoint.join(format!("binding-{host}.json"));
    pinned(&witness_path, ROLE)?;
    let witness = load(&witness_path)?;
    let bundle = portable().join("engine-gap-range-native-idioms-v1");
    let authority = [
        (bundle.join("wire.json"), "wireContractSha256"),
        (bundle.join("hints.json"), "hintsSha256"),
        (endpoint.join("cases.json"), "casesSha256"),
    ];
    let mut portable_inputs = json!({});
    for (path, key) in &authority {
        pinned(path, ROLE)?;
        let digest = sha(path)?;
        require(
            witness[*key] == digest,
            "endpoint evidence differs from portable authority",
        )?;
        portable_inputs[path
            .strip_prefix(assembly::root())?
            .to_str()
            .ok_or("invalid source path")?] = json!(digest);
    }
    let lock_reference = whole().join("gap-range-assembly/package.lock");
    pinned(&lock_reference, COMPACT)?;
    let inputs = work.join("inputs");
    stage(&inputs, &witness)?;
    let manifest = inputs.join(MANIFEST);
    let compiler_dir = args.compiler_target.join("debug");
    let compiler = work.join("oddlang");
    fs::copy(compiler_dir.join("oddlang"), &compiler)?;
    let model = args.original.join(MODEL);
    let probes = [
        model.clone(),
        model.with_file_name("lib.rs"),
        whole().join("gap-range-reference/probe.rs"),
        whole().join("gap-range-record/traits.rs"),
    ];
    require(
        witness["wireProbeSha256"] == sha(&probes[2])?
            && witness["sourceApiProbeSha256"] == sha(&probes[3])?,
        "endpoint caller sources differ",
    )?;
    let cache = work.join("empty-git-cache");
    fs::create_dir(&cache)?;
    let sandbox = Sandbox::new(
        host,
        work,
        &[
            assembly::root(),
            &args.original,
            &args.framework,
            &compiler_dir,
        ],
        &cache,
    )?;
    sandbox.deny(&probes)?;
    let build = |dest: &Path| -> Result<Command> {
        let mut cmd = sandbox.command(work)?;
        cmd.arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&manifest)
            .arg(dest);
        Ok(cmd)
    };
    let product = work.join("product");
    run(&mut build(&product)?)?;
    let lock = product.join("package.lock");
    check_lock(&lock, &manifest, &lock_reference)?;
    let files = derived::package_files(&product)?;
    let mut reference = witness["profiles"]["baseline"]["files"].clone();
    reference
        .as_object_mut()
        .and_then(|o| o.remove("package.lock"))
        .ok_or("missing endpoint package lock")?;
    require(
        files == reference,
        "native package differs from checked endpoint build",
    )?;
    let diagnostics = controls(&inputs.join(BINDING), &manifest, work, build)?;
    let vectors = list(&cases)?
        .iter()
        .map(|row| json!({"id":row["id"],"wire":row["wire"],"type":"range"}))
        .collect::<Vec<_>>();
    require(
        list(&witness["observations"])?.len() == vectors.len(),
        "wrong number of endpoint observations",
    )?;
    let rows = sealed::observe(
        &sandbox,
        work,
        &serde_json::to_vec(&vectors)?,
        &witness["observations"],
        "compact endpoint",
    )?;
    for (path, _) in &authority {
        pinned(path, ROLE)?;
    }
    Ok(
        json!({"format":"marmish-gap-range-compact-source-api-v1","framework":FRAMEWORK,"companion":companion,"host":host,"rustc":version,
        "frameworkBuildToolchain":builder,"compilerSha256":sha(&compiler)?,"verifierSha256":sha(&env::current_exe()?)?,
        "referenceReceiptSha256":sha(&witness_path)?,"previousCompactLockSha256":sha(&lock_reference)?,"manifestSha256":sha(&portable().join(MANIFEST))?,
        "packageLockSha256":sha(&lock)?,"sourceApiSha256":sha(&portable().join(BINDING))?,"portableInputs":portable_inputs,
        "generatedFiles":files,"observations":rows,"rejections":diagnostics,"deniedReads":probes.each_ref().map(|p|p.file_name().unwrap().to_string_lossy()),
        "sourceApiProbeSha256":witness["sourceApiProbeSha256"],"wireProbeSha256":witness["wireProbeSha256"]}),
    )
}
