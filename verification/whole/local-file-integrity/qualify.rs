//! Recover the link-count policy from two native observations and qualify both projections.
use clap::Parser;
use public_data_qualification::{assembly::*, product::verify_release_tree, *};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const BEFORE: &str = "workspace-v7-canonical-mdk-test-seam";
const AFTER: &str = "workspace-v7-local-file-integrity";
const PACKAGE: &str = "native-local-file-integrity-package.json";
const PROFILE: &str = "local-file-integrity-v1";
const MEMBER: &str = "marmish-native";
const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
const CASE: &str = "local_file_observations::descriptor_guard_order";
const PROBE: &str = include_str!("observe.rs");
const ORIGINAL_WRAPPER: &str = r#"
#[cfg(test)]
use self::{safe_file as observed_open, read_bounded as observed_read};
#[cfg(test)]
fn observed_verify(path: &Path, bytes: u64, mode: u32, digest: &[u8; 32], check: bool) -> Result<()> {
    verify_file(path, &Asset { name: "plain", sha256: *digest, bytes, mode }, check)
}
"#;
const GENERATED_WRAPPER: &str = "
#[cfg(test)]
use crate::Error;
#[cfg(test)]
use self::{safe_file as observed_open, read_bounded as observed_read, verify_file as observed_verify};
";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    compiler_target: PathBuf,
    original: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    prepare: bool,
}
fn write_json(path: &Path, mut value: Value) -> Result {
    value.sort_all_objects();
    fs::write(path, json_bytes(&value)?)?;
    Ok(())
}
fn native_cargo(cmd: &mut Command, action: &str, target: &Path) {
    cmd.args([
        "cargo",
        action,
        "--release",
        "--locked",
        "--offline",
        "-p",
        MEMBER,
        "--lib",
        "--target-dir",
    ])
    .arg(target);
}
fn observe(mut cmd: Command, case: &str) -> Result<Value> {
    let raw = stdout(cmd.args(["--", "--exact", case, "--nocapture"]))?;
    let pass = format!("test {case} ... ok");
    require(
        raw.lines().filter(|line| *line == pass).count() == 1
            && raw
                .matches("test result: ok. 1 passed; 0 failed; 0 ignored;")
                .count()
                == 1,
        "expected exactly one passing descriptor test",
    )?;
    let mut result = json!({});
    for (name, prefix, count) in [("guards", "FILE_CASE=", 10), ("links", "LINK_CASE=", 3)] {
        let mut values = serde_json::Map::new();
        for line in raw.lines().filter_map(|line| line.strip_prefix(prefix)) {
            let (key, value) = line
                .split_once(':')
                .ok_or("invalid descriptor observation")?;
            require(
                values.insert(key.into(), value.into()).is_none(),
                "duplicate descriptor observation",
            )?;
        }
        require(values.len() == count, "incomplete descriptor observations")?;
        result[name] = Value::Object(values);
    }
    Ok(result)
}
struct Variant {
    label: &'static str,
    source_digest: String,
    observations: Value,
    recovery: PathBuf,
}
fn recover(cli: &Path, observations: &Value, directory: &Path) -> Result<PathBuf> {
    fs::create_dir(directory)?;
    let observed = directory.join("observed.json");
    let selection = directory.join("selection.json");
    let mut data = observations["links"].clone();
    data["format"] = "oddlang-read-only-file-observation-v1".into();
    write_json(&observed, data)?;
    write_json(
        &selection,
        json!({"format":"oddlang-read-only-file-selection-v1","observationSha256":sha(&observed)?,"profile":load(&portable().join(PROFILE).join("policy.json"))?}),
    )?;
    let recovered = directory.join("recovered");
    run(Command::new(cli)
        .args(["recover", "read-only-file"])
        .arg(observed)
        .arg(selection)
        .arg(&recovered))?;
    Ok(recovered)
}
fn reference(
    original: &Path,
    scratch: &Path,
    target: &Path,
    host: &str,
    cli: &Path,
) -> Result<Vec<Variant>> {
    let repo = scratch.join("original");
    run(Command::new("git")
        .args(["clone", "--quiet", "--local", "--no-checkout"])
        .arg(original)
        .arg(&repo))?;
    run(Command::new("git")
        .current_dir(&repo)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    let path = repo.join(SOURCE);
    let source = fs::read_to_string(&path)?;
    let predicate = "m.nlink() != 1";
    require(
        source.matches(predicate).count() == 1,
        "ambiguous upstream hardlink condition",
    )?;
    let mutant = source.replace(predicate, "!(1..=2).contains(&m.nlink())");
    let mut variants = Vec::new();
    for (label, body, two_links) in [
        ("baseline", source, "descriptor"),
        ("mutated", mutant, "accepted"),
    ] {
        fs::write(&path, format!("{body}{ORIGINAL_WRAPPER}{PROBE}"))?;
        let mut cmd = Command::new("/usr/bin/env");
        build_env(&mut cmd).current_dir(&repo);
        native_cargo(&mut cmd, "test", target);
        cmd.args(["--target", host]);
        let observations = observe(cmd, &format!("native_manifest::{CASE}"))?;
        require(
            observations["links"]
                == json!({"oneLink":"accepted","twoLinks":two_links,"threeLinks":"descriptor"}),
            "unexpected upstream link admission",
        )?;
        let recovery = recover(cli, &observations, &scratch.join(label))?;
        variants.push(Variant {
            label,
            source_digest: bytes_sha(body.as_bytes()),
            observations,
            recovery,
        });
    }
    require(
        variants[0].observations["guards"] == variants[1].observations["guards"],
        "link mutation changed another guard",
    )?;
    Ok(variants)
}
fn compose(inputs: &Path, recovered: &Path) -> Result<usize> {
    let base = portable();
    let mut workspace = load(&base.join(format!("{BEFORE}.json")))?;
    let descriptor = &workspace["members"][MEMBER]["manifest"];
    let name = text(&descriptor["path"])?;
    require(
        pin(&base, name)? == *descriptor,
        "wrong predecessor package pin",
    )?;
    let mut package = load(&base.join(name))?;
    let components = package["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    require(
        components.iter().all(|c| c["module"] != "file_integrity"),
        "duplicate file integrity module",
    )?;
    let graph = format!("{PROFILE}/graph.json");
    let overlay = format!("{PROFILE}/rust.json");
    fs::create_dir_all(inputs.join(PROFILE))?;
    fs::copy(recovered.join("graph.json"), inputs.join(&graph))?;
    if inputs != base {
        fs::copy(base.join(&overlay), inputs.join(&overlay))?;
    }
    let index = components.len();
    components.push(json!({"kind":"read-only-file","module":"file_integrity","reexports":[],"crateReexports":["safe_file","read_bounded","verify_file"],"graph":pin(inputs,&graph)?,"overlay":pin(inputs,&overlay)?}));
    write_json(&inputs.join(PACKAGE), package)?;
    workspace["members"][MEMBER]["manifest"] = pin(inputs, PACKAGE)?;
    write_json(&inputs.join(format!("{AFTER}.json")), workspace)?;
    Ok(index)
}
fn check_lock(product: &Path, baseline: bool) -> Result<String> {
    let path = product.join("workspace.lock");
    let lock = load(&path)?;
    let before = load(&portable().join(BEFORE).join("workspace.lock"))?;
    for key in ["workspaceCargoManifestSha256", "cargoLockSha256"] {
        require(lock[key] == before[key], "workspace Cargo inputs changed")?;
    }
    let members = before["members"]
        .as_object()
        .ok_or("missing predecessor members")?;
    let after = lock["members"]
        .as_object()
        .ok_or("missing generated members")?;
    require(
        members.len() == after.len()
            && members.iter().all(|(name, prior)| {
                after
                    .get(name)
                    .is_some_and(|row| name == MEMBER || row == prior)
            }),
        "unrelated workspace member changed",
    )?;
    if baseline {
        require(
            fs::read(&path)? == fs::read(portable().join(AFTER).join("workspace.lock"))?,
            "baseline lock differs",
        )?;
    }
    verify_release_tree(product, &lock, true, true)?;
    sha(&path)
}
fn qualify(
    sandbox: &Sandbox,
    work: &Path,
    cli: &Path,
    inputs: &Path,
    variant: &Variant,
) -> Result<Value> {
    let index = compose(inputs, &variant.recovery)?;
    let spec = format!("{AFTER}.json");
    let baseline = variant.label == "baseline";
    if baseline {
        require(
            fs::read(inputs.join(&spec))? == fs::read(portable().join(&spec))?,
            "baseline descriptor differs",
        )?;
    }
    let product = work.join(variant.label);
    run(sandbox
        .command(work)?
        .arg(cli)
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(&spec))
        .arg(&product))?;
    let digest = check_lock(&product, baseline)?;
    let target = work.join("target");
    let mut build = sandbox.command(&product)?;
    native_cargo(&mut build, "build", &target);
    run(&mut build)?;
    let entry = product
        .join(MEMBER)
        .join(format!("src/group_{index}/__projection_{index}.rs"));
    write!(
        fs::OpenOptions::new().append(true).open(entry)?,
        "{GENERATED_WRAPPER}{PROBE}"
    )?;
    let mut test = sandbox.command(&product)?;
    native_cargo(&mut test, "test", &target);
    let observations = observe(
        test,
        &format!("file_integrity::__projection_{index}::{CASE}"),
    )?;
    require(
        observations == variant.observations,
        "descriptor observations differ from upstream",
    )?;
    Ok(
        json!({"observations":observations,"workspaceLockSha256":digest,"recovery":load(&variant.recovery.join("recovery.lock"))?}),
    )
}
fn main() -> Result {
    let a = Args::parse();
    let framework = a.framework.canonicalize()?;
    let original = a.original.canonicalize()?;
    checkout(&framework, &a.framework_revision, false)?;
    checkout(&original, ORIGINAL, false)?;
    let revision = stdout(
        Command::new("git")
            .current_dir(root())
            .args(["rev-parse", "HEAD"]),
    )?;
    if !a.prepare {
        checkout(root(), revision.trim(), false)?;
    }
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    require(matches!(host, DARWIN | LINUX), "expected native ARM64")?;
    fs::create_dir_all(&a.compiler_target)?;
    fs::create_dir_all(&a.original_target)?;
    let ct = a.compiler_target.canonicalize()?;
    let target = a.original_target.canonicalize()?;
    run(Command::new("cargo")
        .current_dir(&framework)
        .args([
            "+1.96.1",
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&ct))?;
    let tmp = tempfile::tempdir_in("/tmp")?;
    let scratch = tmp.path().canonicalize()?;
    let compiler = scratch.join("oddlang");
    fs::copy(ct.join("debug/oddlang"), &compiler)?;
    let variants = reference(&original, &scratch, &target, host, &compiler)?;
    if a.prepare {
        compose(&portable(), &variants[0].recovery)?;
        return Ok(());
    }
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let cli = work.join("oddlang");
    fs::copy(&compiler, &cli)?;
    let inputs = work.join("inputs");
    let workspace = stage(&portable().join(format!("{BEFORE}.json")), &inputs)?;
    let cache = work.join("providers");
    providers(&inputs.join(text(&workspace["cargoLock"]["path"])?), &cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[root(), &original, &framework, &target, &ct, &scratch],
        &cache,
    )?;
    sandbox.deny(&[
        original.join(SOURCE),
        scratch.join("original").join(SOURCE),
        whole().join("local-file-integrity/observe.rs"),
    ])?;
    if host == DARWIN {
        use std::os::unix::fs::PermissionsExt;
        let sentinel = scratch.join("external-source");
        fs::write(&sentinel, "[workspace]\n")?;
        let deps = target.join(host).join("release/deps");
        let binary = fs::read_dir(deps)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| {
                p.extension().is_none()
                    && p.metadata()
                        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("marmish_native-"))
            })
            .ok_or("missing upstream native test binary")?;
        sandbox.deny(&[
            whole().join("vault-values/caller.rs"),
            original.join("crates/marmish-vault/src/lib.rs"),
            framework.join("src/lib.rs"),
            binary,
            sentinel,
        ])?;
    }
    let baseline = qualify(&sandbox, &work, &cli, &inputs, &variants[0])?;
    let mutation = qualify(&sandbox, &work, &cli, &inputs, &variants[1])?;
    write_json(
        &a.receipt,
        json!({"format":"marmish-local-file-integrity-v1","original":ORIGINAL,"framework":a.framework_revision,"compilerSha256":sha(&cli)?,"rustc":version.lines().collect::<Vec<_>>(),"companion":revision.trim(),"probeSha256":bytes_sha(PROBE.as_bytes()),"originalAndMutantSourceSha256":variants.iter().map(|v| &v.source_digest).collect::<Vec<_>>(),"deniedSourcePaths":3,"baseline":baseline,"mutation":mutation}),
    )
}
