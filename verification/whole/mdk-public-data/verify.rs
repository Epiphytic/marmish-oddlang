//! Qualify the original and source-recovered MDK error and record with one external caller.
use clap::Parser;
use public_data_qualification::mdk::{PROFILES, here, observe, recover};
use public_data_qualification::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const SOURCE: &str = "crates/marmish-mdk/src/lib.rs";
const INPUTS: &str = "mdk-public-data-v1";
const MANIFEST: &str = "mdk-public-data-package.json";
fn git(root: &Path, args: &[&str]) -> Result<String> {
    stdout(Command::new("git").current_dir(root).args(args))
}
fn clean(root: &Path) -> Result {
    require(
        git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "original checkout has tracked changes",
    )
}
fn mutate(original: &Path, dest: &Path) -> Result {
    run(Command::new("git")
        .args(["clone", "--shared", "--no-checkout"])
        .arg(original)
        .arg(dest))?;
    git(dest, &["checkout", "--detach", ORIGINAL])?;
    let path = dest.join(SOURCE);
    let mut data = fs::read_to_string(&path)?;
    for (before, after) in [
        (
            r#"#[error("response is malformed or truncated")]"#,
            r#"#[error("response was rejected")]"#,
        ),
        (
            "pub struct VersionObservation {\n    pub stdout: String,\n    pub stderr: String,\n}",
            "pub struct VersionObservation {\n    pub stderr: String,\n    pub stdout: String,\n}",
        ),
    ] {
        require(
            data.matches(before).count() == 1,
            "cannot identify the source mutation",
        )?;
        data = data.replacen(before, after, 1);
    }
    fs::write(path, data)?;
    let mut cmd = Command::new("git");
    cmd.current_dir(dest).args([
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--only",
        "-m",
        "Change MDK data and error observations",
        "--",
        SOURCE,
    ]);
    for role in ["AUTHOR", "COMMITTER"] {
        for (key, value) in [
            ("NAME", "Qualification Fixture"),
            ("EMAIL", "fixture@example.invalid"),
            ("DATE", "2026-11-26T12:00:00Z"),
        ] {
            cmd.env(format!("GIT_{role}_{key}"), value);
        }
    }
    run(&mut cmd)?;
    require(
        git(dest, &["diff", "--name-only", ORIGINAL])?
            .lines()
            .eq([SOURCE]),
        "source mutation changed unexpected files",
    )
}
fn recovery(cli: &Path, source: &Path, work: &Path, base: &Path) -> Result<(Value, PathBuf)> {
    let inputs = base.join(INPUTS);
    let facts = recover(cli, source, work, &inputs)?;
    let mut components = Vec::new();
    for (kind, role, symbol) in PROFILES {
        let kind = if role == "record" {
            "public-records"
        } else {
            kind
        };
        components.push(json!({
            "kind":kind,"module":format!("data_{role}"),"visibility":"private","reexports":[symbol],"rootReexports":[symbol],
            "graph":pin(base,&format!("{INPUTS}/{role}.json"))?,"overlay":pin(base,&format!("{INPUTS}/{role}-rust.json"))?
        }));
    }
    fs::copy(
        portable().join(INPUTS).join("Cargo.lock"),
        inputs.join("Cargo.lock"),
    )?;
    let manifest = base.join(MANIFEST);
    fs::write(
        &manifest,
        json_bytes(
            &json!({"format":"oddlang-native-package-v1","name":"marmish-mdk","version":"0.1.0","edition":"2021","cargoLock":pin(base,&format!("{INPUTS}/Cargo.lock"))?,"components":components}),
        )?,
    )?;
    Ok((facts, manifest))
}
struct Build {
    library: PathBuf,
    deps: PathBuf,
    serde: Option<PathBuf>,
}
fn build(workspace: &Path, cache: &Path, snapshot: &Path, original: bool) -> Result<Build> {
    fs::create_dir_all(cache)?;
    let lease = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache.join(".mdk-public-data.lock"))?;
    lease.lock()?;
    let messages = stdout(
        Command::new("cargo")
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "-p",
                "marmish-mdk",
                "--lib",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(cache)
            .current_dir(workspace)
            .env_remove("CARGO_BUILD_TARGET"),
    )?;
    let mut libs = BTreeMap::new();
    for line in messages.lines() {
        let item: Value = serde_json::from_str(line)?;
        if item["reason"] != "compiler-artifact" {
            continue;
        }
        let name = text(&item["target"]["name"])?;
        if !["marmish_mdk", "serde"].contains(&name) {
            continue;
        }
        let files = list(&item["filenames"])?
            .iter()
            .map(text)
            .collect::<Result<Vec<_>>>()?;
        for file in files.into_iter().filter(|f| f.ends_with(".rlib")) {
            require(
                libs.insert(name.to_owned(), PathBuf::from(file)).is_none(),
                "ambiguous Cargo library",
            )?;
        }
    }
    let lib = libs.remove("marmish_mdk").ok_or("missing MDK library")?;
    let serde = libs.remove("serde");
    require(
        serde.is_some() == original,
        "unexpected MDK Serde dependency",
    )?;
    fs::create_dir(snapshot)?;
    let library = snapshot.join("libmarmish_mdk.rlib");
    fs::copy(lib, &library)?;
    Ok(Build {
        library,
        deps: cache.join("debug/deps"),
        serde,
    })
}
#[derive(Parser)]
struct Args {
    oddlang: PathBuf,
    original: PathBuf,
    output: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    original_target: PathBuf,
    #[arg(long)]
    generated_target: PathBuf,
}
fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let cli = args.oddlang.canonicalize()?;
    require(
        git(&original, &["rev-parse", "HEAD"])?.trim() == ORIGINAL,
        "unexpected original revision",
    )?;
    clean(&original)?;
    for target in [&args.original_target, &args.generated_target] {
        fs::create_dir_all(target)?;
    }
    let upstream = args.original_target.canonicalize()?;
    let generated = args.generated_target.canonicalize()?;
    require(
        upstream != generated,
        "original and generated targets must differ",
    )?;
    require(!args.output.exists(), "expected a fresh output directory")?;
    let host = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let mutant = root.join("source-mutant");
    mutate(&original, &mutant)?;
    let mut receipt = json!({"format":"marmish-mdk-public-data-evidence-v1","originalRevision":ORIGINAL,"frameworkRevision":args.framework_revision,"rustc":host.lines().collect::<Vec<_>>(),"probeSha256":sha(&here().join("probe.rs"))?,"lanes":{}});
    let output = std::path::absolute(&args.output)?;
    let reports_dir =
        tempfile::tempdir_in(output.parent().ok_or("missing output parent directory")?)?;
    let reports = reports_dir.path();
    for (lane, reference) in [("baseline", &original), ("mutant", &mutant)] {
        let work = root.join(lane);
        let base = work.join("portable");
        let (facts, manifest) = recovery(&cli, &reference.join(SOURCE), &work, &base)?;
        if lane == "baseline" {
            require(
                fs::read(&manifest)? == fs::read(portable().join(MANIFEST))?,
                "baseline package manifest changed",
            )?;
            for entry in fs::read_dir(base.join(INPUTS))? {
                let entry = entry?;
                require(
                    fs::read(entry.path())?
                        == fs::read(portable().join(INPUTS).join(entry.file_name()))?,
                    "recovered baseline input changed",
                )?;
            }
        }
        let product = work.join("generated");
        run(Command::new(&cli)
            .args(["build", "native-package", "rust"])
            .arg(manifest)
            .arg(&product))?;
        let native = build(
            reference,
            &upstream.join(lane),
            &work.join("original"),
            true,
        )?;
        let candidate = build(
            &product,
            &generated.join(lane),
            &work.join("compiled"),
            false,
        )?;
        let serde = native
            .serde
            .as_deref()
            .ok_or("missing original Serde library")?;
        let observe_build = |b: &Build, name: &str| {
            observe(&b.library, &b.deps, serde, &native.deps, &work.join(name))
        };
        let observed = observe_build(&native, "observe-original")?;
        require(
            observed == observe_build(&candidate, "observe-generated")?,
            format!("{lane} public data observations differ"),
        )?;
        let file = reports.join(format!("{lane}.txt"));
        fs::write(&file, observed.0)?;
        receipt["lanes"][lane] = json!({
            "sourceRevision": git(reference, &["rev-parse", "HEAD"])?.trim(),
            "recovery": facts,
            "packageLockSha256": sha(&product.join("package.lock"))?,
            "outputSha256": sha(&file)?,
            "negativeRustDiagnostics": observed.1
        });
    }
    let before = fs::read_to_string(reports.join("baseline.txt"))?;
    let after = fs::read_to_string(reports.join("mutant.txt"))?;
    let first: Vec<_> = before.lines().collect();
    let second: Vec<_> = after.lines().collect();
    require(
        first.len() == 8
            && second.len() == 8
            && first
                .iter()
                .zip(&second)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| i)
                .eq([4, 7]),
        "unexpected mutant observations",
    )?;
    require(
        first[4] == "InvalidResponse|InvalidResponse|response is malformed or truncated|4"
            && second[4] == "InvalidResponse|InvalidResponse|response was rejected|4"
            && first[7].starts_with("VersionObservation { stdout:")
            && second[7].starts_with("VersionObservation { stderr:"),
        "wrong source mutation outcomes",
    )?;
    clean(&original)?;
    fs::write(reports.join("receipt.json"), json_bytes(&receipt)?)?;
    fs::rename(reports, output)?;
    Ok(())
}
