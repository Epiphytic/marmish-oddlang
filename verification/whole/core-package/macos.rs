//! Rebuild the pinned compiler and qualify the emitted core crate with an independent caller.
use super::{FRAMEWORK, Mutation, ORIGINAL, RUST, checkout, compare, observations, stage, verify};
use public_data_qualification::{
    Result, assembly::seatbelt_policy, capture, json_bytes, portable, product::file_digests,
    require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    io::Write,
    net::{Ipv4Addr, TcpListener, TcpStream},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(clap::Args)]
pub struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

struct BuildEnvironment {
    reads: Vec<PathBuf>,
    vars: BTreeMap<String, OsString>,
}
impl BuildEnvironment {
    fn new(work: &Path, host: &str) -> Result<Self> {
        let tool = PathBuf::from(
            stdout(Command::new("rustc").args(["+1.97.1", "--print", "sysroot"]))?.trim(),
        )
        .canonicalize()?;
        let apple = |args: &[&str]| -> Result<PathBuf> {
            Ok(PathBuf::from(stdout(Command::new("xcrun").args(args))?.trim()).canonicalize()?)
        };
        let developer =
            PathBuf::from(stdout(Command::new("xcode-select").arg("-p"))?.trim()).canonicalize()?;
        let sdk = apple(&["--sdk", "macosx", "--show-sdk-path"])?;
        let clang = apple(&["--sdk", "macosx", "--find", "clang"])?;
        let cache = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| env::home_dir().map(|p| p.join(".cargo")))
            .ok_or("missing Cargo home")?;
        let registry = cache.join("registry").canonicalize()?;
        let home = work.join("candidate-cargo");
        fs::create_dir(&home)?;
        symlink(&registry, home.join("registry"))?;
        let tmp = work.join("tmp");
        fs::create_dir(&tmp)?;
        let vars = BTreeMap::from([
            (
                "PATH".into(),
                env::join_paths([tool.join("bin"), "/usr/bin".into(), "/bin".into()])?,
            ),
            ("HOME".into(), work.into()),
            ("CARGO_HOME".into(), home.into()),
            ("RUSTC".into(), tool.join("bin/rustc").into()),
            ("RUSTDOC".into(), tool.join("bin/rustdoc").into()),
            ("TMPDIR".into(), tmp.into()),
            ("CARGO_NET_OFFLINE".into(), "true".into()),
            ("LC_ALL".into(), "C".into()),
            ("DEVELOPER_DIR".into(), developer.clone().into()),
            ("SDKROOT".into(), sdk.clone().into()),
            (
                format!(
                    "CARGO_TARGET_{}_LINKER",
                    host.to_uppercase().replace('-', "_")
                ),
                clang.clone().into(),
            ),
        ]);
        Ok(Self {
            reads: vec![
                tool,
                developer,
                registry,
                sdk,
                clang.parent().ok_or("missing clang parent")?.into(),
            ],
            vars,
        })
    }
    fn sandbox(&self, work: &Path, denied: &[&Path]) -> Result<Command> {
        let policy = seatbelt_policy(
            work,
            &self.reads.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
            denied,
        )?;
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.args(["-p", &policy])
            .env_clear()
            .envs(&self.vars)
            .current_dir(work);
        Ok(cmd)
    }
}
pub fn environment(work: &Path, host: &str) -> Result {
    let env = BuildEnvironment::new(work, host)?;
    let vars = env
        .vars
        .into_iter()
        .map(|(k, v)| {
            Ok((
                k,
                v.into_string().map_err(|_| "non-UTF-8 environment value")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    println!(
        "{}",
        serde_json::to_string(&json!({"readRoots":env.reads,"environment":vars}))?
    );
    Ok(())
}
fn host_cargo() -> Command {
    let mut cmd = Command::new("cargo");
    for (key, _) in env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("CARGO_PROFILE_")
            || matches!(
                &*name,
                "RUSTC"
                    | "RUSTFLAGS"
                    | "CARGO_ENCODED_RUSTFLAGS"
                    | "RUSTC_BOOTSTRAP"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "CARGO_BUILD_TARGET"
            )
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("RUSTUP_TOOLCHAIN", "1.97.1")
        .env("CARGO_NET_OFFLINE", "true")
        .arg("+1.97.1");
    cmd
}
fn rejected(cmd: &mut Command, reason: &str) -> Result {
    let r = capture(cmd, 600, None)?;
    require(
        !r.status.success() && String::from_utf8_lossy(&r.stderr).contains(reason),
        format!(
            "expected rejection containing {reason}, got {}",
            String::from_utf8_lossy(&r.stderr)
        ),
    )
}
struct Projection<'a> {
    work: &'a Path,
    env: BuildEnvironment,
    denied: Vec<&'a Path>,
    compiler: PathBuf,
}
impl Projection<'_> {
    fn command(&self) -> Result<Command> {
        self.env.sandbox(self.work, &self.denied)
    }
    fn deny_read(&self, path: &Path) -> Result {
        require(path.try_exists()?, "missing source isolation canary")?;
        rejected(
            self.command()?
                .arg(if path.is_dir() { "/bin/ls" } else { "/bin/cat" })
                .arg(path),
            "Operation not permitted",
        )
    }
    fn generate_command(&self, inputs: &Path, out: &Path) -> Result<Command> {
        let mut cmd = self.command()?;
        cmd.arg(&self.compiler)
            .args(["build", "native-package", "rust"])
            .arg(inputs.join("core-package.json"))
            .arg(out);
        Ok(cmd)
    }
    fn generate(&self, inputs: &Path, out: &Path) -> Result {
        run(&mut self.generate_command(inputs, out)?)?;
        verify(out, &inputs.join("core-package.json"))
    }
    fn cargo(&self) -> Result<Command> {
        let mut cmd = self.command()?;
        cmd.arg("cargo");
        Ok(cmd)
    }
}
fn cargo(
    mut cmd: Command,
    dir: &Path,
    target: &Path,
    action: &str,
    transcript: Option<&Path>,
) -> Result<String> {
    cmd.current_dir(dir)
        .args([action, "--release", "--locked", "--offline", "--target-dir"])
        .arg(target)
        .args(["--package", "marmish-core"]);
    if let Some(path) = transcript {
        cmd.args(["--test", "differential"])
            .env("ODDLANG_CORE_TRANSCRIPT", path);
    }
    stdout(&mut cmd)
}
fn test_counts(log: &str) -> Result<Vec<[usize; 3]>> {
    let pattern =
        regex::Regex::new(r"(?m)^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored")?;
    pattern
        .captures_iter(log)
        .map(|c| Ok([c[1].parse()?, c[2].parse()?, c[3].parse()?]))
        .collect()
}
fn install(caller: &Path, package: &Path) -> Result {
    let tests = package.join("tests");
    fs::create_dir_all(&tests)?;
    fs::copy(caller, tests.join("differential.rs"))?;
    Ok(())
}

pub fn qualify(args: Args) -> Result {
    require(cfg!(target_os = "macos"), "macOS Seatbelt required")?;
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    checkout(&original, ORIGINAL)?;
    checkout(&framework, FRAMEWORK)?;
    let info = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        info.lines()
            .any(|line| line == format!("commit-hash: {RUST}")),
        "unexpected compiler commit",
    )?;
    let host = info
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let target = work.join("framework-target");
    run(host_cargo()
        .current_dir(&framework)
        .args([
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&target))?;
    let companion = portable()
        .ancestors()
        .nth(2)
        .ok_or("missing companion root")?
        .canonicalize()?;
    let mut project = Projection {
        work: &work,
        env: BuildEnvironment::new(&work, host)?,
        compiler: target.join("debug/oddlang"),
        denied: vec![&original, &companion, &framework],
    };
    for root in &project.denied {
        project.deny_read(&root.join("Cargo.toml"))?;
    }
    let external = tempfile::tempdir()?;
    let canary = external.path().canonicalize()?.join("config.rs");
    fs::copy(original.join("crates/marmish-core/src/config.rs"), &canary)?;
    project.deny_read(&canary)?;
    run(project.command()?.args(["/bin/cat", "/dev/null"]))?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    TcpStream::connect(address)?;
    rejected(
        project.command()?.args([
            "/usr/bin/nc",
            "-v",
            "-z",
            "-w",
            "1",
            "127.0.0.1",
            &address.port().to_string(),
        ]),
        "Operation not permitted",
    )?;
    let inputs = work.join("inputs");
    let projected = work.join("projected");
    stage(&portable(), &inputs, None)?;
    project.generate(&inputs, &projected)?;
    require(
        fs::read(projected.join("package.lock"))?
            == fs::read(portable().join("core-package-v1/package.lock"))?,
        "generated lock differs",
    )?;
    let duplicate = work.join("duplicate");
    project.generate(&inputs, &duplicate)?;
    let outputs = file_digests(&projected)?;
    require(
        outputs == file_digests(&duplicate)?,
        "repeated projection differs",
    )?;
    let generated_target = work.join("generated-target");
    cargo(
        project.cargo()?,
        &projected,
        &generated_target,
        "build",
        None,
    )?;
    let bad = work.join("wrong-digest");
    let bad_out = work.join("wrong-digest-output");
    stage(&portable(), &bad, Some(Mutation::StaleFrameDigest))?;
    rejected(
        &mut project.generate_command(&bad, &bad_out)?,
        "digest mismatch",
    )?;
    require(!bad_out.try_exists()?, "invalid package published")?;
    let reference = work.join("reference");
    fs::create_dir(&reference)?;
    let archive = work.join("original.tar");
    run(Command::new("git")
        .current_dir(&original)
        .args(["archive", ORIGINAL, "-o"])
        .arg(&archive))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&reference))?;
    fs::remove_file(archive)?;
    let reference_target = work.join("reference-target");
    let original_rows = work.join("original.jsonl");
    let generated_rows = work.join("generated.jsonl");
    project.denied.extend([
        reference.as_path(),
        reference_target.as_path(),
        original_rows.as_path(),
    ]);
    let core = reference.join("crates/marmish-core");
    let contracts = core.join("tests/contracts.rs");
    project.deny_read(&contracts)?;
    let tests = projected.join("tests");
    fs::create_dir(&tests)?;
    fs::copy(&contracts, tests.join("contracts.rs"))?;
    require(
        sha(&contracts)? == sha(&tests.join("contracts.rs"))?,
        "upstream tests changed",
    )?;
    let orig = test_counts(&cargo(
        host_cargo(),
        &reference,
        &reference_target,
        "test",
        None,
    )?)?;
    let emitted = test_counts(&cargo(
        project.cargo()?,
        &projected,
        &generated_target,
        "test",
        None,
    )?)?;
    require(
        orig == emitted
            && orig.iter().map(|r| r[0]).sum::<usize>() == 7
            && orig.iter().all(|r| r[1..] == [0, 0]),
        "upstream test results differ",
    )?;
    let caller = whole().join("core-package/caller.rs");
    install(&caller, &core)?;
    install(&caller, &projected)?;
    cargo(
        host_cargo(),
        &reference,
        &reference_target,
        "test",
        Some(&original_rows),
    )?;
    project.deny_read(&original_rows)?;
    project.deny_read(&reference_target)?;
    cargo(
        project.cargo()?,
        &projected,
        &generated_target,
        "test",
        Some(&generated_rows),
    )?;
    let counts = compare(&original_rows, &generated_rows)?;
    let baseline = observations(&fs::read(&original_rows)?)?;
    let mut mutants = json!({});
    for (mutation, name, family, case, field, expected) in [
        (
            Mutation::RetentionBound,
            "retention-bound",
            "retention",
            "Some(31536000)",
            "error",
            json!("RetentionOutOfRange"),
        ),
        (
            Mutation::RelayRuleOrder,
            "relay-rule-order",
            "relay",
            "raw-whitespace-before-at",
            "error",
            json!("DisallowedComponents"),
        ),
        (
            Mutation::FrameVersion,
            "frame-version",
            "decode",
            "tuple-good",
            "error",
            json!("InvalidVersion"),
        ),
        (
            Mutation::FrameErrorOrder,
            "frame-error-order",
            "frame-error",
            "SizeLimit",
            "ordinal",
            json!(1),
        ),
    ] {
        let input = work.join(format!("{name}-inputs"));
        let out = work.join(format!("{name}-crate"));
        let transcript = work.join(format!("{name}.jsonl"));
        stage(&portable(), &input, Some(mutation))?;
        project.generate(&input, &out)?;
        install(&caller, &out)?;
        cargo(
            project.cargo()?,
            &out,
            &work.join(format!("{name}-target")),
            "test",
            Some(&transcript),
        )?;
        let rows = observations(&fs::read(&transcript)?)?;
        let key = (family.to_owned(), case.to_owned());
        let before = baseline.get(&key).ok_or("missing baseline witness")?;
        let after = rows.get(&key).ok_or("missing mutation witness")?;
        require(
            after.get(field) == Some(&expected) && after != before,
            format!("mutation survived: {name}"),
        )?;
        mutants[name] = json!({"case":[family,case],"original":before,"mutated":after,"packageLockSha256":sha(&out.join("package.lock"))?,"transcriptSha256":sha(&transcript)?});
    }
    let mut receipt: Value = json!({
        "originalRevision":ORIGINAL,"frameworkRevision":FRAMEWORK,"compilerCommit":RUST,"host":host,
        "originalCargoLockSha256":sha(&original.join("Cargo.lock"))?,
        "sourceIsolation":{"provider":"macOS Seatbelt allowlist","deniedReadCanaries":7,"networkDenied":true,"deniedConnectCanary":true,"referenceCopiedAfterReleaseBuild":true},
        "originalPublicSource":file_digests(&core.join("src"))?,"unchangedTestSha256":sha(&contracts)?,
        "unchangedTestsPassedPerImplementation":7,"unchangedTestGroupCounts":orig,"callerSha256":sha(&caller)?,
        "packageManifestSha256":sha(&inputs.join("core-package.json"))?,"generatedRelease":outputs,
        "twoGenerationsEqual":true,"differentialCounts":counts,"differentialTranscriptSha256":sha(&original_rows)?,
        "mutations":mutants,"rejectedGraphDigestDrift":true
    });
    receipt.sort_all_objects();
    tmp.close()?;
    let bytes = json_bytes(&receipt)?;
    let parent = args
        .receipt
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.persist(args.receipt)?;
    println!(
        "Qualified {} matching native observations",
        counts.values().sum::<usize>()
    );
    Ok(())
}
