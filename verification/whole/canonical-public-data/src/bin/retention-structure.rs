use clap::Parser;
use public_data_qualification::{
    Result, json_bytes, list, load, portable, require, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const REV: &str = "af54339d250290490d62d4475a77539b7e739071";
const RUST: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
const TEST: &str = "crates/marmish-core/tests/contracts.rs";
const OLD: &str = "retention must be between one second and 365 days";
const NEW: &str = "retention must be between one second and seven days";

#[derive(Parser)]
struct Args {
    oddlang: PathBuf,
    original: PathBuf,
    output: PathBuf,
    #[arg(long)]
    framework_revision: String,
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    for (key, _) in env::vars_os() {
        let k = key.to_string_lossy();
        if ["RUST", "CARGO_ENCODED_", "CARGO_TARGET_", "CARGO_PROFILE_"]
            .iter()
            .any(|p| k.starts_with(p))
            || k == "CARGO_BUILD_TARGET"
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("RUSTUP_TOOLCHAIN", "1.97.1");
    cmd
}

fn write(path: &Path, mut value: Value) -> Result {
    value.sort_all_objects();
    Ok(fs::write(path, json_bytes(&value)?)?)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    stdout(command("git").current_dir(root).args(args))
}

fn mutant(original: &Path, root: &Path) -> Result {
    stdout(
        command("git")
            .args(["clone", "--shared", "--no-checkout"])
            .arg(original)
            .arg(root),
    )?;
    git(root, &["checkout", "--detach", REV])?;
    let config = root.join("crates/marmish-core/src/config.rs");
    let source = fs::read_to_string(&config)?;
    require(
        source.matches("365 * 86_400").count() == 1 && source.matches(OLD).count() == 1,
        "retention mutation anchors changed",
    )?;
    fs::write(
        config,
        source
            .replace("365 * 86_400", "7 * 86_400")
            .replace(OLD, NEW),
    )?;
    stdout(
        command("git")
            .current_dir(root)
            .args([
                "-c",
                "user.name=Structural Control",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-am",
                "Change retained interval",
            ])
            .env("GIT_AUTHOR_DATE", "2026-11-26T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-11-26T00:00:00Z"),
    )?;
    Ok(())
}

fn generated(cli: &Path, stage: &Path, graph: &Path) -> Result<PathBuf> {
    let source = portable();
    let mut manifest = load(&source.join("core-package.json"))?;
    let components = list(&manifest["components"])?;
    let inputs = stage.join("inputs");
    for pin in std::iter::once(&manifest["cargoLock"])
        .chain(components.iter().flat_map(|r| [&r["graph"], &r["overlay"]]))
    {
        let rel = text(&pin["path"])?;
        let file = source.join(rel);
        require(
            sha(&file)? == text(&pin["sha256"])?,
            format!("pin mismatch for {rel}"),
        )?;
        let dst = inputs.join(rel);
        fs::create_dir_all(dst.parent().unwrap())?;
        fs::copy(file, dst)?;
    }
    let index = components
        .iter()
        .position(|r| r["kind"] == "value-domain")
        .ok_or("missing value-domain")?;
    let pin = &mut manifest["components"][index]["graph"];
    fs::copy(graph, inputs.join(text(&pin["path"])?))?;
    pin["sha256"] = json!(sha(graph)?);
    let package = inputs.join("package.json");
    write(&package, manifest)?;
    let out = stage.join("generated");
    stdout(
        command(cli)
            .args(["build", "native-package", "rust"])
            .arg(package)
            .arg(&out),
    )?;
    Ok(out)
}

fn exercise(
    manifest: &Path,
    work: &Path,
    triple: &str,
    caller: &Path,
) -> Result<(Vec<String>, String)> {
    let target = work.join("target");
    let result = stdout(
        command("cargo")
            .args([
                "+1.97.1",
                "test",
                "--release",
                "--offline",
                "--locked",
                "--manifest-path",
            ])
            .arg(manifest)
            .arg("--target-dir")
            .arg(&target)
            .args([
                "--target",
                triple,
                "-p",
                "marmish-core",
                "--test",
                "contracts",
            ]),
    )?;
    let re = regex::Regex::new(r"(?m)^test (\w+) \.\.\. ok$")?;
    let mut tests = re
        .captures_iter(&result)
        .map(|c| c[1].to_owned())
        .collect::<Vec<_>>();
    tests.sort();
    require(
        tests.len() == 7,
        format!("unexpected contract tests: {result}"),
    )?;
    let deps = target.join(triple).join("release/deps");
    let mut libraries = vec![];
    for entry in fs::read_dir(&deps)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("libmarmish_core-") && name.ends_with(".rlib") {
            libraries.push(entry.path());
        }
    }
    require(libraries.len() == 1, "expected a single core rlib")?;
    let exe = work.join("caller");
    stdout(
        command("rustc")
            .args([
                "+1.97.1",
                "--edition",
                "2021",
                "--target",
                triple,
                "--extern",
            ])
            .arg(format!("marmish_core={}", libraries[0].display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-L")
            .arg(format!(
                "dependency={}",
                target.join("release/deps").display()
            ))
            .arg(caller)
            .arg("-o")
            .arg(&exe),
    )?;
    Ok((tests, stdout(command(exe).current_dir(work))?))
}

fn main() -> Result {
    let args = Args::parse();
    let cli = args.oddlang.canonicalize()?;
    let source = args.original.canonicalize()?;
    let version = stdout(command("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        version.lines().any(|l| l == format!("commit-hash: {RUST}")),
        "wrong rustc commit",
    )?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing host")?;
    require(
        git(&source, &["rev-parse", "HEAD"])?.trim() == REV
            && git(&source, &["status", "--porcelain"])?.is_empty(),
        "expected pinned clean upstream",
    )?;
    let here = whole().join("retention-structure");
    let caller = here.join("caller.rs");
    let unchanged = fs::read(source.join(TEST))?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&args.output)?;
    let out = args.output.canonicalize()?;
    let stage = tempfile::Builder::new()
        .prefix("retention-structure-")
        .tempdir()?;
    let modified = stage.path().join("mutant");
    mutant(&source, &modified)?;
    let mut cases = json!({});
    for (label, checkout) in [("baseline", &source), ("mutated", &modified)] {
        let case = stage.path().join(label);
        fs::create_dir(&case)?;
        let rev = git(checkout, &["rev-parse", "HEAD"])?;
        let mut selection = load(&here.join("selection.json"))?;
        selection["sourceRevision"] = json!(rev.trim());
        let selection_path = case.join("selection.json");
        write(&selection_path, selection)?;
        let recovered = out.join(label);
        let manifest = checkout.join("crates/marmish-core/Cargo.toml");
        stdout(
            command(&cli)
                .args(["recover", "value-domain-structure"])
                .arg(&manifest)
                .arg(selection_path)
                .arg(&recovered),
        )?;
        let graph = recovered.join("domain.ttl");
        let content = fs::read_to_string(&graph)?;
        if label == "baseline" {
            require(
                content == fs::read_to_string(portable().join("core-config-v1/domain.ttl"))?,
                "baseline graph drift",
            )?;
        } else {
            require(
                content.contains("c:maximum 604800") && content.contains(NEW),
                "mutated graph lost interval or diagnostic",
            )?;
        }
        let projected = generated(&cli, &case, &graph)?;
        fs::create_dir(projected.join("tests"))?;
        fs::write(projected.join("tests/contracts.rs"), &unchanged)?;
        require(
            fs::read(checkout.join(TEST))? == unchanged,
            "upstream test drift",
        )?;
        let (tests, actual) = exercise(&manifest, &case.join("original"), host, &caller)?;
        let candidate = exercise(
            &projected.join("Cargo.toml"),
            &case.join("candidate"),
            host,
            &caller,
        )?;
        require(
            tests == candidate.0 && actual == candidate.1,
            "candidate tests or downstream transcript differ",
        )?;
        let expected = if label == "baseline" {
            "Some(31536000) => Some(31536000)".to_string()
        } else {
            format!("Some(31536000) => RetentionOutOfRange: {NEW}")
        };
        require(actual.contains(&expected), "wrong retention boundary")?;
        let output = recovered.join("caller.txt");
        fs::write(&output, actual)?;
        cases[label] = json!({"sourceRevision":rev.trim(),"tests":tests,"graphSha256":sha(&graph)?,"callerOutputSha256":sha(&output)?,"packageLockSha256":sha(&projected.join("package.lock"))?});
    }
    require(
        cases["baseline"]["callerOutputSha256"] != cases["mutated"]["callerOutputSha256"],
        "mutation invisible to caller",
    )?;
    require(
        git(&source, &["status", "--porcelain"])?.is_empty(),
        "upstream checkout modified",
    )?;
    write(
        &out.join("receipt.json"),
        json!({"frameworkRevision":args.framework_revision,"originalRevision":REV,"compilerCommit":RUST,"target":host,"unchangedTestSha256":sha(&source.join(TEST))?,"callerSha256":sha(&caller)?,"cases":cases}),
    )
}
