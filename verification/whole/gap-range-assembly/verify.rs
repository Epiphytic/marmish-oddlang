//! Compare the compact two-component package against the independently qualified explicit package.
use super::{ID, MODEL, PACKAGE, REASON_DIR, RECORD_DIR, qualify};
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox},
    capture, json_bytes, load, portable,
    product::pinned,
    require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "2e2c37e53ed52676017b25e3143eca48a0daff13";
const SEALED: &str = "6355ae8504a7eb64ae01fc0591f29e3879c38128";
const RECOVERED: &str = "fb3199bcbdb334c1914228f56efa99fb00dd3d42";
const MANIFEST: &str = "engine-gap-range-derived-package.json";

#[derive(clap::Args)]
pub(super) struct Args {
    framework: PathBuf,
    compiler_target: PathBuf,
    original: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    package_lock: Option<PathBuf>,
}

pub(super) fn stage(inputs: &Path) -> Result {
    let base = portable();
    let spec = load(&base.join(MANIFEST))?;
    let parts = spec["components"].as_array().ok_or("missing components")?;
    require(parts.len() == 2, "expected two recovered components")?;
    let cargo = assembly::pinned_copy(&base, &spec["cargoLock"], inputs)?;
    pinned(&cargo, RECOVERED)?;
    for (component, directory) in parts.iter().zip([REASON_DIR, RECORD_DIR]) {
        let fields = component.as_object().ok_or("invalid component")?;
        let reason = directory == REASON_DIR;
        require(
            fields.len() == if reason { 2 } else { 1 }
                && (!reason || fields.contains_key("module"))
                && component["recovery"]["path"] == format!("{directory}/recovery.lock"),
            "expected compact component with pinned recovery",
        )?;
        let lock = assembly::pinned_copy(&base, &component["recovery"], inputs)?;
        if reason {
            pinned(&lock, RECOVERED)?;
        } else {
            let witness =
                whole().join("gap-range-recovery/aarch64-apple-darwin/baseline-recovery.lock");
            pinned(&witness, SEALED)?;
            require(
                fs::read(lock)? == fs::read(witness)?,
                "range lock differs from sealed recovery",
            )?;
        }
        for name in ["graph.json", "rust.json"] {
            let source = base.join(directory).join(name);
            pinned(&source, RECOVERED)?;
            fs::copy(source, inputs.join(directory).join(name))?;
        }
    }
    fs::copy(base.join(MANIFEST), inputs.join(MANIFEST))?;
    Ok(())
}

fn recoveries(manifest: &Path, lock_path: &Path, reference: &Value) -> Result<Value> {
    let mut lock = load(lock_path)?;
    require(
        lock["manifestSha256"] == sha(manifest)?,
        "lock omitted compact manifest digest",
    )?;
    lock["manifestSha256"] = reference["manifestSha256"].clone();
    let spec = load(manifest)?;
    let parts = lock["components"]
        .as_array_mut()
        .ok_or("invalid package lock")?;
    require(parts.len() == 2, "expected both locked components")?;
    let mut rows = Vec::new();
    for (index, (component, kind)) in parts.iter_mut().zip(["reason", "range"]).enumerate() {
        let fields = component.as_object_mut().ok_or("invalid lock component")?;
        let hash = fields
            .remove("recoverySha256")
            .ok_or("missing recovery digest")?;
        let id = fields
            .remove("recoveredDeclaration")
            .ok_or("missing recovered declaration")?;
        require(
            hash == spec["components"][index]["recovery"]["sha256"] && id == format!("{ID}{kind}"),
            "recovered component provenance differs",
        )?;
        rows.push(json!({"sha256":hash,"declaration":id}));
    }
    require(
        lock == *reference,
        "effective package differs from sealed reference",
    )?;
    Ok(json!(rows))
}

fn package_files(root: &Path, dir: &Path, hashes: &mut Value) -> Result {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            package_files(root, &path, hashes)?;
        } else {
            require(kind.is_file(), "expected a regular package file")?;
            let name = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("invalid package filename")?;
            if name != "package.lock" {
                hashes[name] = json!(sha(&path)?);
            }
        }
    }
    Ok(())
}

fn verify(work: &Path, args: &Args, host: &str, builder: &Value, companion: &str) -> Result<Value> {
    let reference_path = portable().join(RECORD_DIR).join("package.lock");
    let explicit = portable().join(PACKAGE);
    let archive = whole()
        .join("gap-range-recovery")
        .join(host)
        .join("receipt.json");
    for path in [&reference_path, &explicit, &archive] {
        pinned(path, SEALED)?;
    }
    let receipt = load(&archive)?;
    let baseline = &receipt["cases"]["baseline"];
    let reference = load(&reference_path)?;
    require(
        receipt["host"] == host
            && receipt["original"] == ORIGINAL
            && baseline["packageLockSha256"] == sha(&reference_path)?
            && baseline["manifestSha256"] == sha(&explicit)?
            && baseline["generatedFiles"] == reference["generatedFiles"],
        "sealed reference provenance differs",
    )?;
    let compiler_dir = args.compiler_target.join("debug");
    let compiler = work.join("oddlang");
    fs::copy(compiler_dir.join("oddlang"), &compiler)?;
    let inputs = work.join("inputs");
    stage(&inputs)?;
    let manifest = inputs.join(MANIFEST);
    let model = args.original.join(MODEL);
    let forbidden = [
        ("explicit-manifest", explicit),
        ("independent-package-lock", reference_path.clone()),
        ("native-probe", whole().join("gap-range-reference/probe.rs")),
        ("original-crate-root", model.with_file_name("lib.rs")),
        ("original-model", model),
    ];
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
    sandbox.deny(&forbidden.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>())?;
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
    let rows = recoveries(&manifest, &lock, &reference)?;
    let mut expected = reference["generatedFiles"].clone();
    expected["Cargo.toml"] = reference["cargoManifestSha256"].clone();
    expected["Cargo.lock"] = reference["cargoLockSha256"].clone();
    let mut files = json!({});
    package_files(&product, &product, &mut files)?;
    require(
        files == expected,
        "package files differ from sealed package",
    )?;
    let mut diagnostics = json!({});
    for name in ["graph.json", "recovery.lock"] {
        let relative = format!("{RECORD_DIR}/{name}");
        let path = inputs.join(&relative);
        let original = fs::read(&path)?;
        fs::OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(b"\n")?;
        let dest = work.join("rejected");
        let rejected = capture(&mut build(&dest)?, 600, None)?;
        let diagnostic = String::from_utf8(rejected.stderr)?;
        require(
            !rejected.status.success()
                && dest.symlink_metadata().is_err()
                && diagnostic.trim() == format!("input digest mismatch: {relative}"),
            "tampered input did not produce expected rejection",
        )?;
        diagnostics[relative] = json!(diagnostic.trim());
        fs::write(path, original)?;
    }
    Ok(
        json!({"format":"marmish-gap-range-derived-package-v1","host":host,"compilerSha256":sha(&compiler)?,
        "framework":FRAMEWORK,"frameworkBuildToolchain":builder,"companion":companion,"verifierSha256":sha(&env::current_exe()?)?,
        "sealedCompanion":SEALED,"referenceReceiptSha256":sha(&archive)?,"referencePackageLockSha256":sha(&reference_path)?,
        "manifestSha256":sha(&manifest)?,"assembledPackageLockSha256":sha(&lock)?,"recoveries":rows,
        "components":reference["components"],"files":files,"deniedReads":forbidden.map(|(label,_)|label),"tamperDiagnostics":diagnostics}),
    )
}

pub(super) fn qualify(mut args: Args) -> Result {
    args.original = args.original.canonicalize()?;
    args.framework = args.framework.canonicalize()?;
    args.compiler_target = assembly::resolved(&std::path::absolute(&args.compiler_target)?)?;
    assembly::checkout(&args.original, ORIGINAL, false)?;
    assembly::checkout(&args.framework, FRAMEWORK, false)?;
    let companion = stdout(
        Command::new("git")
            .current_dir(assembly::root())
            .args(["rev-parse", "HEAD"]),
    )?;
    require(
        run(Command::new("git").current_dir(assembly::root()).args([
            "status",
            "--porcelain",
            "--untracked-files=no",
        ]))?
        .is_empty(),
        "expected clean tracked companion inputs",
    )?;
    let host = qualify::host()?;
    let builder = qualify::version("+1.96.1", qualify::BUILDER, host)?;
    run(qualify::tool("cargo")
        .current_dir(&args.framework)
        .env("CARGO_INCREMENTAL", "0")
        .args([
            "+1.96.1",
            "build",
            "--offline",
            "--locked",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&args.compiler_target))?;
    let scratch = tempfile::tempdir()?;
    let work = scratch.path().canonicalize()?;
    let mut result = verify(&work, &args, host, &builder, companion.trim())?;
    result.sort_all_objects();
    if let Some(dest) = args.package_lock {
        fs::copy(work.join("product/package.lock"), dest)?;
    }
    fs::write(args.receipt, json_bytes(&result)?)?;
    Ok(())
}
