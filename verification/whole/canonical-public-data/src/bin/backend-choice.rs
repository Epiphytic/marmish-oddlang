use clap::{Args, Parser, Subcommand};
use public_data_qualification::{
    Result, capture, json_bytes, load, portable, require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PIN: &str = "af54339d250290490d62d4475a77539b7e739071";
const COMPONENT: &str = "native-backend-choice-v1";
const MANIFEST: &str = "native-backend-choice-package.json";
const SOURCE: &str = "crates/marmish-native/src/identity.rs";

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Observe(Observer),
    Qualify(Qualification),
}
#[derive(Args)]
struct Observer {
    native: PathBuf,
    deps: PathBuf,
    serde: PathBuf,
    serde_deps: PathBuf,
    scratch: PathBuf,
}
#[derive(Args)]
struct Qualification {
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

impl Observer {
    fn compiler(&self) -> Command {
        let mut cmd = Command::new("rustc");
        cmd.args(["+1.97.1", "--edition=2024"])
            .arg("-L")
            .arg(format!("dependency={}", self.deps.display()))
            .arg("-L")
            .arg(format!("dependency={}", self.serde_deps.display()))
            .arg("--extern")
            .arg(format!("marmish_native={}", self.native.display()))
            .arg("--extern")
            .arg(format!("serde={}", self.serde.display()));
        cmd
    }

    fn observe(&self) -> Result<Value> {
        fs::create_dir(&self.scratch)?;
        let binary = self.scratch.join("probe");
        run(self
            .compiler()
            .arg(whole().join("backend-choice/probe.rs"))
            .arg("-o")
            .arg(&binary))?;
        let output = stdout(&mut Command::new(&binary))?;
        let mut diagnostics = json!({});
        let source = self.scratch.join("negative.rs");
        let mut check = |name: &str, expected: &str, body: &str| -> Result {
            fs::write(&source, body)?;
            let result = capture(
                self.compiler()
                    .arg("--error-format=json")
                    .arg(&source)
                    .arg("-o")
                    .arg(&binary),
                600,
                None,
            )?;
            let mut codes = BTreeSet::new();
            for line in std::str::from_utf8(&result.stderr)?.lines() {
                let diagnostic: Value = serde_json::from_str(line)?;
                if let Some(code) = diagnostic["code"]["code"].as_str() {
                    codes.insert(code.to_owned());
                }
            }
            require(
                !result.status.success() && codes.len() == 1 && codes.contains(expected),
                format!(
                    "{name}: expected {expected}, got {codes:?} ({})",
                    result.status
                ),
            )?;
            diagnostics[name] = json!(expected);
            Ok(())
        };
        for (name, bound) in [
            ("Debug", "std::fmt::Debug"),
            ("Display", "std::fmt::Display"),
            ("Default", "Default"),
            ("Hash", "std::hash::Hash"),
            ("Ord", "Ord"),
            ("PartialOrd", "PartialOrd"),
            ("Serialize", "serde::Serialize"),
            ("Deserialize", "for<'de> serde::Deserialize<'de>"),
        ] {
            check(
                name,
                "E0277",
                &format!(
                    "fn require<T: {bound}>() {{}} fn main() {{ require::<marmish_native::identity::BackendChoice>(); }}"
                ),
            )?;
        }
        for (name, code, body) in [
            (
                "integerConversion",
                "E0277",
                "fn require<T: From<BackendChoice>>() {} fn main(){require::<u8>();}",
            ),
            (
                "crossTypeEquality",
                "E0277",
                "fn require<T: PartialEq<BackendChoice>>() {} fn main(){require::<u8>();}",
            ),
            (
                "incompleteMatch",
                "E0004",
                "fn main(){match BackendChoice::File {BackendChoice::Auto => (), BackendChoice::Keychain => ()}}",
            ),
        ] {
            check(
                name,
                code,
                &format!("use marmish_native::identity::BackendChoice; {body}"),
            )?;
        }
        check(
            "rootExposure",
            "E0432",
            "use marmish_native::BackendChoice; fn main() {}",
        )?;
        Ok(json!({"outputs":output,"negative":diagnostics}))
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    stdout(Command::new("git").arg("-C").arg(root).args(args))
}
fn clean_original(root: &Path) -> Result {
    require(
        git(root, &["rev-parse", "HEAD"])?.trim() == PIN
            && git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "original tracked tree or revision changed",
    )
}
fn mutate(original: &Path, root: &Path) -> Result {
    run(Command::new("git")
        .args(["clone", "--shared", "--no-checkout"])
        .arg(original)
        .arg(root))?;
    git(root, &["checkout", "--detach", PIN])?;
    let path = root.join(SOURCE);
    let before = fs::read_to_string(&path)?;
    let decl = "pub enum BackendChoice {\n    Auto,\n    Keychain,\n    File,\n}";
    require(
        before.matches(decl).count() == 1,
        "expected one enum declaration",
    )?;
    fs::write(
        path,
        before.replace(
            decl,
            "pub enum BackendChoice {\n    File,\n    Auto,\n    Keychain,\n}",
        ),
    )?;
    let mut cmd = Command::new("git");
    for role in ["AUTHOR", "COMMITTER"] {
        for (key, val) in [
            ("NAME", "Qualification Fixture"),
            ("EMAIL", "fixture@example.invalid"),
            ("DATE", "2026-11-26T12:00:00Z"),
        ] {
            cmd.env(format!("GIT_{role}_{key}"), val);
        }
    }
    run(cmd.arg("-C").arg(root).args([
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--only",
        "-m",
        "Rotate native choice declarations",
        "--",
        SOURCE,
    ]))?;
    require(
        git(root, &["diff", "--name-only", PIN])?.trim() == SOURCE,
        "unexpected mutation paths",
    )
}

fn projection(cli: &Path, source: &Path, work: &Path, baseline: bool) -> Result<(PathBuf, Value)> {
    let selection = whole().join("backend-choice/selection.json");
    let mut config = load(&selection)?;
    config["sourceSha256"] = json!(sha(source)?);
    let selected = work.join("selection.json");
    fs::write(&selected, json_bytes(&config)?)?;
    let extracted = work.join("extracted");
    run(Command::new(cli)
        .args(["recover", "plain-choice"])
        .arg(source)
        .arg(&selected)
        .arg(&extracted))?;
    let base = work.join("portable");
    let assets = base.join(COMPONENT);
    fs::create_dir_all(&assets)?;
    fs::copy(
        portable().join(COMPONENT).join("Cargo.lock"),
        assets.join("Cargo.lock"),
    )?;
    for name in ["graph.json", "rust.json"] {
        fs::copy(extracted.join(name), assets.join(name))?;
        if baseline {
            require(
                fs::read(assets.join(name))? == fs::read(portable().join(COMPONENT).join(name))?,
                format!("changed baseline {name}"),
            )?;
        }
    }
    let pin = |name: &str| public_data_qualification::pin(&base, &format!("{COMPONENT}/{name}"));
    let manifest = json!({
        "format": "oddlang-native-package-v1",
        "name": "marmish-native",
        "version": "0.3.1",
        "edition": "2024",
        "cargoLock": pin("Cargo.lock")?,
        "components": [{
            "kind": "checked-values", "module": "identity", "visibility": "public",
            "graph": pin("graph.json")?, "overlay": pin("rust.json")?,
            "reexports": ["BackendChoice"], "rootReexports": []
        }]
    });
    let spec = base.join(MANIFEST);
    fs::write(&spec, json_bytes(&manifest)?)?;
    if baseline {
        require(
            fs::read(&selected)? == fs::read(selection)?
                && fs::read(&spec)? == fs::read(portable().join(MANIFEST))?,
            "baseline selection or package manifest changed",
        )?;
    }
    let generated = work.join("generated");
    run(Command::new(cli)
        .args(["build", "native-package", "rust"])
        .arg(spec)
        .arg(&generated))?;
    Ok((generated, load(&extracted.join("observation.json"))?))
}

struct Built {
    native: PathBuf,
    deps: PathBuf,
    serde: Option<PathBuf>,
}
fn build(package: &Path, target: &Path, snapshot: &Path, original: bool) -> Result<Built> {
    fs::create_dir_all(target)?;
    let lease = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(target.join(".backend-choice.lock"))?;
    lease.lock()?;
    let artifacts = stdout(
        Command::new("cargo")
            .current_dir(package)
            .env("MARMISH_BUILD_COMMIT", PIN)
            .env_remove("CARGO_BUILD_TARGET")
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "-p",
                "marmish-native",
                "--lib",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target),
    )?;
    let mut libs = BTreeMap::new();
    for line in artifacts.lines() {
        let row: Value = serde_json::from_str(line)?;
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let name = row["target"]["name"]
            .as_str()
            .ok_or("missing target name")?;
        if !matches!(name, "marmish_native" | "serde") {
            continue;
        }
        for p in row["filenames"]
            .as_array()
            .ok_or("missing filenames")?
            .iter()
            .filter_map(Value::as_str)
            .filter(|p| p.ends_with(".rlib"))
        {
            require(
                libs.insert(name.to_owned(), PathBuf::from(p)).is_none(),
                format!("duplicate {name} rlib"),
            )?;
        }
    }
    let serde = libs.remove("serde");
    require(
        original == serde.is_some(),
        "unexpected Serde artifact selection",
    )?;
    fs::copy(
        libs.get("marmish_native").ok_or("missing native rlib")?,
        snapshot,
    )?;
    Ok(Built {
        native: snapshot.to_owned(),
        deps: target.join("debug/deps"),
        serde,
    })
}

fn qualify(args: Qualification) -> Result {
    let original = args.original.canonicalize()?;
    clean_original(&original)?;
    fs::create_dir_all(&args.original_target)?;
    fs::create_dir_all(&args.generated_target)?;
    let upstream_target = args.original_target.canonicalize()?;
    let generated_target = args.generated_target.canonicalize()?;
    require(
        upstream_target != generated_target,
        "expected separate build targets",
    )?;
    fs::create_dir(&args.output)?;
    let temp = tempfile::tempdir()?;
    let mutant = temp.path().join("mutant-source");
    mutate(&original, &mutant)?;
    let mut lanes = json!({});
    for (lane, src, ordinals) in [
        ("baseline", original.as_path(), [0, 1, 2]),
        ("mutant", mutant.as_path(), [1, 2, 0]),
    ] {
        let work = temp.path().join(lane);
        fs::create_dir(&work)?;
        let (generated, recovery) = projection(
            &args.oddlang.canonicalize()?,
            &src.join(SOURCE),
            &work,
            lane == "baseline",
        )?;
        let upstream = build(
            src,
            &upstream_target.join(lane),
            &work.join("libupstream.rlib"),
            true,
        )?;
        let candidate = build(
            &generated,
            &generated_target.join(lane),
            &work.join("libcandidate.rlib"),
            false,
        )?;
        let serde = upstream.serde.as_ref().ok_or("missing Serde rlib")?;
        let observe = |built: &Built, name: &str| {
            Observer {
                native: built.native.clone(),
                deps: built.deps.clone(),
                serde: serde.clone(),
                serde_deps: upstream.deps.clone(),
                scratch: work.join(name),
            }
            .observe()
        };
        let observed = observe(&upstream, "original")?;
        require(
            observed == observe(&candidate, "candidate")?,
            "candidate disagrees with original",
        )?;
        let expected: String = ["Auto", "Keychain", "File"]
            .into_iter()
            .zip(ordinals)
            .map(|(name, i)| format!("{name}|{i}|{i}|{i}\n"))
            .collect();
        require(
            observed["outputs"] == expected,
            "unexpected discriminant or cast output",
        )?;
        let output = args.output.join(format!("{lane}.txt"));
        fs::write(
            &output,
            observed["outputs"].as_str().ok_or("missing probe output")?,
        )?;
        lanes[lane] = json!({
            "sourceRevision": git(src, &["rev-parse", "HEAD"])?.trim(),
            "recovery": recovery,
            "assembledLockSha256": sha(&generated.join("package.lock"))?,
            "outputSha256": sha(&output)?,
            "negativeRustDiagnostics": observed["negative"]
        });
    }
    clean_original(&original)?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let receipt = json!({
        "format": "marmish-native-backend-choice-evidence-v1",
        "originalRevision": PIN,
        "frameworkRevision": args.framework_revision,
        "rustc": version.lines().collect::<Vec<_>>(),
        "probeSha256": sha(&whole().join("backend-choice/probe.rs"))?,
        "lanes": lanes
    });
    fs::write(args.output.join("receipt.json"), json_bytes(&receipt)?)?;
    Ok(())
}

fn main() -> Result {
    match Cli::parse().command {
        Action::Qualify(args) => qualify(args),
        Action::Observe(args) => {
            println!("{}", args.observe()?);
            Ok(())
        }
    }
}
