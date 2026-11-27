//! Qualify linked range recovery with a controlled source mutation and independent native callers.
use super::{LEGACY, MODEL, evidence, pinned, publish, recover, sealed, witnesses, write_json};
use public_data_qualification::{
    Result,
    assembly::{self, COMPILER, DARWIN, LINUX, ORIGINAL},
    capture, load, portable, require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "9923e6664899de66915ed3a94e417f261ffed401";
pub(super) const BUILDER: &str = "31fca3adb283cc9dfd56b49cdee9a96eb9c96ffd";

#[derive(clap::Args)]
pub(super) struct Args {
    #[arg(long)]
    framework_revision: String,
    framework: PathBuf,
    compiler_target: PathBuf,
    original: PathBuf,
    rustdoc: PathBuf,
    output: PathBuf,
}

pub(super) fn tool(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
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
    cmd
}

pub(super) fn version(toolchain: &str, revision: &str, host: &str) -> Result<Value> {
    let info = stdout(tool("rustc").args([toolchain, "-vV"]))?;
    for expected in [format!("host: {host}"), format!("commit-hash: {revision}")] {
        require(
            info.lines().any(|line| line == expected),
            "unexpected native Rust toolchain",
        )?;
    }
    Ok(json!(info.lines().collect::<Vec<_>>()))
}

fn mutate(original: &Path, dest: &Path) -> Result<PathBuf> {
    run(Command::new("git")
        .args(["clone", "--quiet", "--local", "--no-checkout"])
        .arg(original)
        .arg(dest))?;
    run(Command::new("git")
        .current_dir(dest)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    assembly::checkout(dest, ORIGINAL, false)?;
    let path = dest.join(MODEL);
    let body = fs::read_to_string(&path)?;
    const BEFORE: &str = "#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]\npub struct MissingRange {\n";
    require(
        body.matches(BEFORE).count() == 1,
        "cannot isolate the source record mutation",
    )?;
    fs::write(
        &path,
        body.replace(
            BEFORE,
            &BEFORE.replace("pub struct", "#[serde(deny_unknown_fields)]\npub struct"),
        ),
    )?;
    Ok(path)
}

fn check_graphs(work: &Path) -> Result {
    let authored = portable().join("engine-gap-range-v1");
    let graph = authored.join("graph.json");
    let overlay = authored.join("rust.json");
    pinned(&graph, LEGACY)?;
    pinned(&overlay, LEGACY)?;
    let expected = load(&graph)?;
    let mut mutant = expected.clone();
    let field = mutant
        .pointer_mut("/modules/0/records/0/readPolicy/unknownFields")
        .ok_or("missing unknown field policy")?;
    require(
        *field == "ignore",
        "unexpected authored unknown field policy",
    )?;
    *field = json!("reject");
    let rust = load(&overlay)?;
    for (label, graph) in [("baseline", &expected), ("mutation", &mutant)] {
        let lane = work.join(label).join("recovered");
        require(
            load(&lane.join("graph.json"))? == *graph,
            format!("unexpected {label} record graph"),
        )?;
        require(
            load(&lane.join("rust.json"))? == rust,
            format!("unexpected {label} Rust overlay"),
        )?;
    }
    Ok(())
}

pub(super) fn host() -> Result<&'static str> {
    let platform = stdout(Command::new("uname").args(["-sm"]))?;
    Ok(match platform.trim() {
        "Darwin arm64" => DARWIN,
        "Linux aarch64" => LINUX,
        _ => return Err("expected a native ARM64 host".into()),
    })
}

pub(super) fn qualify(args: Args) -> Result {
    require(
        args.framework_revision == FRAMEWORK,
        "unexpected range recovery compiler revision",
    )?;
    let framework = args.framework.canonicalize()?;
    let original = args.original.canonicalize()?;
    let rustdoc = args.rustdoc.canonicalize()?;
    let target = assembly::resolved(&std::path::absolute(args.compiler_target)?)?;
    let out = assembly::resolved(&std::path::absolute(args.output)?)?;
    for path in [&target, &out] {
        require(
            [assembly::root(), &framework, &original]
                .iter()
                .all(|root| !path.starts_with(root)),
            "use external compiler and output directories",
        )?;
    }
    require(
        !target.starts_with(&out) && !out.starts_with(&target) && out.symlink_metadata().is_err(),
        "use a fresh evidence destination separate from the compiler target",
    )?;
    let companion = stdout(
        Command::new("git")
            .current_dir(assembly::root())
            .args(["rev-parse", "HEAD"]),
    )?;
    let companion = companion.trim();
    let check_trees = || -> Result {
        for (tree, rev) in [
            (assembly::root(), companion),
            (framework.as_path(), FRAMEWORK),
            (original.as_path(), ORIGINAL),
        ] {
            assembly::checkout(tree, rev, false)?;
        }
        Ok(())
    };
    check_trees()?;
    let host = host()?;
    let rustc = version("+1.97.1", COMPILER, host)?;
    let builder = version("+1.96.1", BUILDER, host)?;
    run(tool("cargo")
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
        .arg(&target))?;
    let compiler = target.join("debug/oddlang");
    let scratch = tempfile::tempdir()?;
    let work = scratch.path().canonicalize()?;
    let mutant = work.join("original-mutant");
    let source = mutate(&original, &mutant)?;
    let root_sha = sha(&original.join(MODEL).with_file_name("lib.rs"))?;
    require(
        sha(&source.with_file_name("lib.rs"))? == root_sha,
        "mutation changed crate root",
    )?;
    let checked = || evidence::check(&rustdoc, &original, &source, host);
    let evidence = checked()?;
    let witnesses = witnesses::check(&original, &source, host, &rustc)?;
    for (label, model) in [
        ("baseline", original.join(MODEL)),
        ("mutation", source.clone()),
    ] {
        let lane = work.join(label);
        fs::create_dir(&lane)?;
        recover(
            &compiler,
            &model,
            &rustdoc.join(format!("{label}.json")),
            &lane,
        )?;
    }
    let denied = work.join("stale-selector-output");
    let refusal = capture(
        tool(&compiler)
            .args(["recover", "serde-public-record"])
            .arg(&source)
            .arg(rustdoc.join("mutation.json"))
            .arg(work.join("baseline/selection.json"))
            .arg(&denied),
        600,
        None,
    )?;
    require(
        !refusal.status.success()
            && denied.symlink_metadata().is_err()
            && String::from_utf8_lossy(&refusal.stderr).contains("digest"),
        "stale selector did not produce the expected digest refusal",
    )?;
    check_graphs(&work)?;
    let packages = sealed::check(
        sealed::Inputs {
            compiler: compiler.clone(),
            framework: framework.clone(),
            original: original.clone(),
            scratch: work.clone(),
            baseline_doc: rustdoc.join("baseline.json"),
            mutation_doc: rustdoc.join("mutation.json"),
            host: host.into(),
        },
        &witnesses["observations"],
    )?;
    require(
        checked()? == evidence,
        "Rustdoc inputs changed during qualification",
    )?;
    require(
        run(Command::new("git").current_dir(&mutant).args([
            "status",
            "--porcelain",
            "--untracked-files=all",
        ]))? == format!(" M {MODEL}\n").as_bytes(),
        "controlled mutation changed during observation",
    )?;
    check_trees()?;
    publish(out, |dest| {
        let mut cases = packages["cases"].clone();
        for (label, policy) in [("baseline", "ignore"), ("mutation", "reject")] {
            let lane = work.join(label);
            let output = dest.join(label);
            fs::create_dir(&output)?;
            for entry in fs::read_dir(lane.join("recovered"))? {
                let file = entry?;
                fs::copy(file.path(), output.join(file.file_name()))?;
            }
            fs::copy(lane.join("selection.json"), output.join("selection.json"))?;
            for (key, file) in [
                ("selectionSha256", "selection.json"),
                ("recoveryLockSha256", "recovery.lock"),
                ("graphSha256", "graph.json"),
                ("overlaySha256", "rust.json"),
            ] {
                cases[label][key] = json!(sha(&output.join(file))?);
            }
            cases[label]["unknownFields"] = json!(policy);
        }
        let reference = whole().join("gap-range-reference");
        write_json(
            &dest.join("receipt.json"),
            json!({
                "format":"marmish-linked-range-recovery-v1","framework":FRAMEWORK,"companion":companion,
                "compilerSha256":sha(&compiler)?,"compilerBuildToolchain":builder,"linkedChoiceRecovery":witnesses["linkedChoice"],
                "original":ORIGINAL,"rustc":rustc,"host":host,"rustdocEvidence":evidence,"crateRootSha256":root_sha,
                "nativeAuthoredReceiptSha256":witnesses["nativeReceiptSha256"],"originalReferenceSha256":witnesses["originalReferenceSha256"],
                "callerSha256":sha(&reference.join("probe.rs"))?,"casesSha256":sha(&reference.join("cases.json"))?,
                "traitsSha256":sha(&whole().join("gap-range-record/traits.rs"))?,"verifierSha256":sha(&env::current_exe()?)?,
                "scopeSourceDenials":packages["scopeSourceDenials"],"staleSelectorRejected":true,"changedNativeRows":["unknown-field"],"cases":cases
            }),
        )
    })
}
