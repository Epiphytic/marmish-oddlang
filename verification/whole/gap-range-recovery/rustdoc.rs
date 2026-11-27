//! Collect the two independently compiled rustdoc documents used by linked-range recovery.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::bytes_sha, cli_projection::REVISION, json_bytes, load, require, run, sha,
    stdout,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Map, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const MODEL: &str = "crates/marmish-engine/src/model.rs";
const BEFORE: &str =
    "#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]\npub struct MissingRange {\n";
const PROFILE: [(&str, &str); 3] = [
    ("RUSTC_BOOTSTRAP", "1"),
    ("CARGO_PROFILE_DEV_DEBUG", "0"),
    ("CARGO_INCREMENTAL", "0"),
];

#[derive(Parser)]
struct Args {
    original: PathBuf,
    target: PathBuf,
    output: PathBuf,
}

fn git(tree: &Path, args: &[&str]) -> Result<String> {
    stdout(Command::new("git").current_dir(tree).args(args))
}

fn clean(tree: &Path) -> Result<()> {
    require(
        git(tree, &["rev-parse", "HEAD"])?.trim_end() == REVISION
            && git(tree, &["status", "--porcelain", "--untracked-files=all"])?.is_empty(),
        "expected a clean pinned original checkout",
    )
}

fn tool(name: &str) -> Command {
    let mut cmd = Command::new(name);
    cmd.env_clear();
    for key in [
        "HOME",
        "PATH",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "SDKROOT",
    ] {
        if let Some(value) = env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd.envs(PROFILE);
    cmd
}

// Resolve an external destination even when some trailing directories do not exist.
fn destination(path: &Path) -> Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let name = path.file_name().ok_or("expected directory name")?;
            let parent = path.parent().ok_or("missing parent")?;
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            Ok(destination(parent)?.join(name))
        }
        Err(e) => Err(e.into()),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    clean(&original)?;
    let output = destination(&args.output)?;
    let target = destination(&args.target)?;
    require(
        !output.starts_with(&original)
            && !target.starts_with(&original)
            && !output.starts_with(&target)
            && !target.starts_with(&output),
        "use separate external target and evidence directories",
    )?;
    require(
        output.symlink_metadata().is_err(),
        "evidence destination exists",
    )?;
    let version = stdout(tool("rustdoc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing rustdoc host")?;
    require(
        version
            .lines()
            .any(|s| s == "commit-hash: 8bab26f4f68e0e26f0bb7960be334d5b520ea452"),
        "unexpected rustdoc revision",
    )?;
    let parent = output.parent().ok_or("missing evidence parent")?;
    fs::create_dir_all(parent)?;
    let scratch = tempfile::tempdir_in(parent)?;
    let product = scratch.path().join("evidence");
    fs::create_dir(&product)?;
    let mutant = scratch.path().join("original-mutant");
    run(Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--local",
            "--no-hardlinks",
            "--no-checkout",
        ])
        .arg(&original)
        .arg(&mutant))?;
    git(&mutant, &["checkout", "--quiet", "--detach", REVISION])?;
    clean(&mutant)?;
    let source = mutant.join(MODEL);
    let body = fs::read_to_string(&source)?;
    require(
        body.matches(BEFORE).count() == 1,
        "cannot isolate the source record mutation",
    )?;
    fs::write(
        &source,
        body.replacen(
            BEFORE,
            &BEFORE.replace("pub struct", "#[serde(deny_unknown_fields)]\npub struct"),
            1,
        ),
    )?;
    let recipe = [
        "cargo",
        "+1.97.1",
        "rustdoc",
        "--locked",
        "--offline",
        "--lib",
        "-p",
        "marmish-engine",
        "--target",
        host,
        "--target-dir",
        "<target-dir>",
        "--",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
        "--document-private-items",
    ];
    let mut rows = Map::new();
    for (label, tree) in [("baseline", &original), ("mutation", &mutant)] {
        let source = tree.join(MODEL);
        let before = sha(&source)?;
        let mut cmd = tool("cargo");
        for arg in &recipe[1..] {
            if *arg == "<target-dir>" {
                cmd.arg(&target);
            } else {
                cmd.arg(arg);
            }
        }
        run(cmd.current_dir(tree))?;
        let result = target.join(host).join("doc/marmish_engine.json");
        let document = load(&result)?;
        let root = document["root"]
            .as_u64()
            .ok_or("missing rustdoc root")?
            .to_string();
        require(
            document["format_version"] == 57
                && document["includes_private"] == true
                && document["target"]["triple"] == host
                && document["index"][root]["name"] == "marmish_engine"
                && sha(&source)? == before,
            "unexpected rustdoc profile or source",
        )?;
        let name = format!("{label}.json");
        let dest = product.join(&name);
        fs::copy(result, &dest)?;
        rows.insert(
            label.to_string(),
            json!({"sourceSha256":before,"rustdocSha256":sha(&dest)?,"rustdocFile":name}),
        );
    }
    clean(&original)?;
    require(
        git(&mutant, &["status", "--porcelain", "--untracked-files=all"])?
            == format!(" M {MODEL}\n"),
        "unexpected changes in mutant checkout",
    )?;
    require(
        rows["baseline"]["rustdocSha256"] != rows["mutation"]["rustdocSha256"],
        "rustdoc did not distinguish mutation",
    )?;
    let receipt = json!({"format":"marmish-linked-range-rustdoc-v2","original":REVISION,"host":host,
         "source":MODEL,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,
         "crateRootSha256":sha(&original.join(MODEL).with_file_name("lib.rs"))?,
         "rustdoc":version.lines().collect::<Vec<_>>(),"runnerSha256":bytes_sha(include_bytes!("rustdoc.rs")),
         "cargoArgs":recipe.as_slice(),"profileEnvironment":PROFILE.into_iter().collect::<std::collections::BTreeMap<_,_>>(),"inputs":rows});
    fs::write(product.join("evidence.json"), json_bytes(&receipt)?)?;
    renameat_with(CWD, &product, CWD, &output, RenameFlags::NOREPLACE)?;
    Ok(())
}
