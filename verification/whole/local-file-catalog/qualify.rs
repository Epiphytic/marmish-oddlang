//! Qualify the read-only directory catalog against the upstream test and independent inert corpus.
use clap::Parser;
use public_data_qualification::{assembly::*, product::verify_release_tree, *};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const MEMBER: &str = "marmish-native";
const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
const CASE: &str = "versioned_directory_discovery_supports_both_families_and_rejects_ambiguity";
const OBS: &str = "inert_catalog_guard_observations";
const SAMPLE_SHA: &str = "52f7f2f6dc0c0da5451eb11d5219824654b0b19b9204d91d9df40d3f564a25fd";
const TEST_SHA: &str = "7daed3cc184642817d58cd5e56d620ce578a8124504d7c112c2f11324c5189f0";
const WORKSPACE: &str = "workspace-v7-local-file-catalog";
const PACKAGE: &str = "native-local-file-catalog-package.json";
const GRAPH: &str = "local-file-catalog-v1/graph.json";
const WRAPPER: &str = "
#[cfg(test)]
mod tests {
    use super::{AssetNaming, NAMES, verify_directory};
    use crate::native_manifest::{Manifest, TARGETS};
    use sha2::{Digest, Sha256};
    use std::fs;
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
}

fn original_parts(source: &str) -> Result<(&str, &str)> {
    let marker = "    fn sample() -> String {";
    let test = format!("    #[test]\n    fn {CASE}()");
    require(
        source.matches(marker).count() == 1 && source.matches(&test).count() == 1,
        "ambiguous original catalog test",
    )?;
    let source = source
        .strip_suffix("\n}\n")
        .ok_or("unexpected test module boundary")?;
    let sample = &source[source.find(marker).ok_or("missing sample")?..];
    let helper = &sample[..sample
        .find("    #[test]\n")
        .ok_or("missing test after sample")?];
    let body = &source[source.find(&test).ok_or("missing test case")?..];
    require(
        bytes_sha(helper.as_bytes()) == SAMPLE_SHA && bytes_sha(body.as_bytes()) == TEST_SHA,
        "original catalog test digest mismatch",
    )?;
    Ok((helper, body))
}

fn cargo_case(mut cmd: Command, case: &str, target: &Path, host: Option<&str>) -> Command {
    cmd.args([
        "cargo",
        "test",
        "--locked",
        "--offline",
        "--release",
        "-p",
        MEMBER,
        "--lib",
        "--target-dir",
    ])
    .arg(target);
    if let Some(triple) = host {
        cmd.arg("--target").arg(triple);
    }
    cmd.args(["--", "--exact", case]);
    cmd
}

fn observations(cmd: &mut Command, case: &str) -> Result<Value> {
    let output = stdout(cmd.arg("--nocapture"))?;
    require(
        output
            .lines()
            .filter(|l| *l == format!("test {case} ... ok"))
            .count()
            == 1
            && output
                .matches("test result: ok. 1 passed; 0 failed; 0 ignored;")
                .count()
                == 1,
        "named catalog test did not execute exactly once",
    )?;
    let mut cases = json!({});
    for row in output
        .lines()
        .filter_map(|line| line.strip_prefix("CATALOG_CASE="))
    {
        let (key, value) = row.split_once(':').ok_or("invalid catalog observation")?;
        require(cases.get(key).is_none(), "duplicate catalog observation")?;
        cases[key] = value.into();
    }
    Ok(cases)
}

fn corpus(
    command: impl Fn() -> Result<Command>,
    module: &str,
    target: &Path,
    host: Option<&str>,
) -> Result<Value> {
    let case = format!("{module}::{CASE}");
    observations(&mut cargo_case(command()?, &case, target, host), &case)?;
    let case = format!("{module}::{OBS}");
    let rows = observations(&mut cargo_case(command()?, &case, target, host), &case)?;
    require(
        rows.as_object().is_some_and(|map| map.len() == 12),
        "incomplete catalog corpus",
    )?;
    Ok(rows)
}

fn project(sandbox: &Sandbox, cli: &Path, spec: &Path, out: &Path) -> Result<Vec<u8>> {
    run(sandbox
        .command(spec.parent().ok_or("missing input directory")?)?
        .arg(cli)
        .args(["build", "native-workspace", "rust"])
        .arg(spec)
        .arg(out))?;
    let lock = fs::read(out.join("workspace.lock"))?;
    verify_release_tree(out, &serde_json::from_slice(&lock)?, true, true)?;
    Ok(lock)
}

fn attach(path: &Path, helper: &str, body: &str, probe: &str) -> Result {
    let mut f = fs::OpenOptions::new().append(true).open(path)?;
    write!(f, "{WRAPPER}{helper}{body}\n{probe}\n}}\n")?;
    Ok(())
}

fn write_sorted(path: &Path, mut data: Value) -> Result {
    data.sort_all_objects();
    fs::write(path, json_bytes(&data)?)?;
    Ok(())
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
    checkout(root(), revision.trim(), false)?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    require(
        matches!(host, DARWIN | LINUX),
        "requires a native ARM64 host",
    )?;
    let scratch = tempfile::tempdir_in("/tmp")?;
    let outside = scratch.path().canonicalize()?;
    run(Command::new("cargo")
        .args([
            "+1.97.1",
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(whole().join("canonical-public-data/Cargo.toml"))
        .arg("--target-dir")
        .arg(outside.join("tools"))
        .args(["--bin", "compose-local-file-catalog", "--"])
        .arg(&original)
        .arg("--check"))?;
    fs::create_dir_all(&a.compiler_target)?;
    fs::create_dir_all(&a.original_target)?;
    let compiler_target = a.compiler_target.canonicalize()?;
    let original_target = a.original_target.canonicalize()?;
    run(Command::new("cargo")
        .current_dir(&framework)
        .args([
            "+1.96.1",
            "build",
            "--offline",
            "--locked",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    let upstream = outside.join("original");
    run(Command::new("git")
        .args(["clone", "--quiet", "--local", "--no-checkout"])
        .arg(&original)
        .arg(&upstream))?;
    run(Command::new("git")
        .current_dir(&upstream)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    let src = fs::read_to_string(upstream.join(SOURCE))?;
    let (helper, body) = original_parts(&src)?;
    let probe_path = whole().join("local-file-catalog/observe.rs");
    let probe = fs::read_to_string(&probe_path)?;
    fs::write(
        upstream.join(SOURCE),
        format!(
            "{}\n{probe}\n}}\n",
            src.strip_suffix("}\n")
                .ok_or("missing upstream module end")?
        ),
    )?;
    let expected = corpus(
        || {
            let mut cmd = Command::new("/usr/bin/env");
            build_env(&mut cmd).current_dir(&upstream);
            Ok(cmd)
        },
        "native_manifest::tests",
        &original_target,
        Some(host),
    )?;

    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let cli = work.join("oddlang");
    fs::copy(compiler_target.join("debug/oddlang"), &cli)?;
    let inputs = work.join("inputs");
    let name = format!("{WORKSPACE}.json");
    let spec = inputs.join(&name);
    let mut workspace = stage(&portable().join(name), &inputs)?;
    let cache = work.join("providers");
    providers(&inputs.join(text(&workspace["cargoLock"]["path"])?), &cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[
            root(),
            &original,
            &framework,
            &outside,
            &original_target,
            &compiler_target,
        ],
        &cache,
    )?;
    sandbox.deny(&[
        original.join(SOURCE),
        upstream.join(SOURCE),
        probe_path.clone(),
    ])?;
    if host == DARWIN {
        use std::os::unix::fs::PermissionsExt;
        let deps = original_target.join(host).join("release/deps");
        let mut binaries = Vec::new();
        for item in fs::read_dir(deps)? {
            let path = item?.path();
            if path.extension().is_none()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|s| s.starts_with("marmish_native-"))
                && path.is_file()
                && path.metadata()?.permissions().mode() & 0o111 != 0
            {
                binaries.push(path);
            }
        }
        binaries.sort();
        let sentinel = outside.join("source-denial-probe");
        fs::write(&sentinel, "[workspace]\n")?;
        sandbox.deny(&[
            whole().join("vault-values/caller.rs"),
            original.join("crates/marmish-vault/src/lib.rs"),
            framework.join("src/lib.rs"),
            binaries
                .into_iter()
                .next()
                .ok_or("missing upstream native test executable")?,
            sentinel,
        ])?;
    }
    let product = work.join("product");
    let lock = project(&sandbox, &cli, &spec, &product)?;
    require(
        lock == fs::read(portable().join(WORKSPACE).join("workspace.lock"))?,
        "catalog workspace lock mismatch",
    )?;
    let mut package = load(&inputs.join(PACKAGE))?;
    let components = list(&package["components"])?;
    let mut catalog = components
        .iter()
        .enumerate()
        .filter(|(_, c)| c["kind"] == "local-file-catalog");
    let (index, component) = catalog.next().ok_or("missing catalog")?;
    require(catalog.next().is_none(), "duplicate catalog")?;
    let group = components
        .iter()
        .position(|c| c["module"] == component["module"])
        .ok_or("missing module group")?;
    let leaf = format!("{MEMBER}/src/group_{group}/__projection_{index}.rs");
    let module = format!("native_manifest::__projection_{index}::tests");
    let target = work.join("cache");
    run(sandbox
        .command(&product)?
        .args([
            "cargo",
            "build",
            "--locked",
            "--offline",
            "--release",
            "-p",
            MEMBER,
            "--lib",
            "--target-dir",
        ])
        .arg(&target))?;
    attach(&product.join(&leaf), helper, body, &probe)?;
    let cases = corpus(|| sandbox.command(&product), &module, &target, None)?;
    require(
        cases == expected,
        "generated catalog guard observations differ from original",
    )?;

    let mut graph = load(&inputs.join(GRAPH))?;
    let removed = graph["prefixes"]
        .as_array_mut()
        .and_then(Vec::pop)
        .ok_or("no catalog prefix to remove")?;
    write_sorted(&inputs.join(GRAPH), graph)?;
    package["components"][index]["graph"] = pin(&inputs, GRAPH)?;
    write_sorted(&inputs.join(PACKAGE), package)?;
    workspace["members"][MEMBER]["manifest"] = pin(&inputs, PACKAGE)?;
    write_sorted(&spec, workspace)?;
    let mutant = work.join("mutation");
    let changed = project(&sandbox, &cli, &spec, &mutant)?;
    require(
        changed != lock,
        "catalog mutation did not change workspace lock",
    )?;
    let path = mutant.join(&leaf);
    attach(&path, helper, body, "")?;
    let source = fs::read_to_string(path)?;
    let test = source
        .find(&format!("fn {CASE}()"))
        .ok_or("missing unchanged case")?;
    let decoy = test
        + source[test..]
            .find("fs::write(root.path().join(\"marmish-0.3.invalid.manifest\")")
            .ok_or("missing ambiguity case")?;
    let check = decoy
        + source[decoy..]
            .find("assert!(verify_directory(")
            .ok_or("missing guard assertion")?;
    require(
        check
            < decoy
                + source[decoy..]
                    .find("fs::remove_file(&path)")
                    .ok_or("missing cleanup")?,
        "wrong ambiguity assertion",
    )?;
    let line = source[..check].bytes().filter(|b| *b == b'\n').count() + 1;
    let column = source[..check]
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .count()
        + 1;
    let case = format!("{module}::{CASE}");
    let result = capture(
        &mut cargo_case(sandbox.command(&mutant)?, &case, &target, None),
        600,
        None,
    )?;
    let transcript = String::from_utf8(result.stdout)?;
    require(result.status.code()==Some(101) && transcript.contains(&format!("test {case} ... FAILED")) &&
        transcript.contains(&format!("panicked at {leaf}:{line}:{column}:\nassertion failed: verify_directory(root.path(), AssetNaming::VersionedDownloads, pin).is_err()")) &&
        transcript.contains("test result: FAILED. 0 passed; 1 failed; 0 ignored;"),"removed catalog family did not fail at the original ambiguity assertion")?;
    write_sorted(
        &a.receipt,
        json!({"format":"marmish-local-verified-file-catalog-v1","original":ORIGINAL,"framework":a.framework_revision,
        "compilerSha256":sha(&cli)?,"companion":revision.trim(),"rustc":version.lines().collect::<Vec<_>>(),
        "workspaceLockSha256":bytes_sha(&lock),"sourceDenials":3,"case":format!("native_manifest::tests::{CASE}"),"caseSha256":TEST_SHA,"sampleSha256":SAMPLE_SHA,
        "namedPassed":1,"probeSha256":sha(&probe_path)?,"observations":cases,"mutation":{"removedPrefix":removed,"workspaceLockSha256":bytes_sha(&changed),"namedFailed":1,"failure":"missed-ambiguity"}}),
    )
}
