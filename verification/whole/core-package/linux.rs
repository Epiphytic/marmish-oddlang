//! Qualify a graph-only package before mounting its original source and callers.
use public_data_qualification::{Result, json_bytes, require, run, sha, stdout, whole};
use serde_json::json;
use std::{
    env,
    ffi::OsString,
    fs,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use super::{FRAMEWORK, ORIGINAL, RUST, checkout};
#[derive(clap::Args)]
pub struct Args {
    portable: PathBuf,
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

fn connect(port: u16) -> std::io::Result<TcpStream> {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(3),
    )
}
pub fn probe_loopback(port: u16) -> Result {
    match connect(port) {
        Err(e)
            if matches!(
                e.raw_os_error(),
                Some(libc::ECONNREFUSED | libc::ENETUNREACH | libc::EHOSTUNREACH | libc::EPERM)
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
        Ok(_) => Err("isolated process reached host loopback listener".into()),
    }
}

struct Sandbox {
    elevated: bool,
    common: Vec<OsString>,
    drop: Vec<OsString>,
}
impl Sandbox {
    fn new(work: &Path, toolchain: &Path, registry: &Path, uid: u32, gid: u32) -> Result<Self> {
        let elevated = !Command::new("bwrap")
            .args(["--unshare-all", "--ro-bind", "/", "/", "/bin/true"])
            .output()?
            .status
            .success();
        let mut common: Vec<OsString> = [
            "--unshare-net",
            "--unshare-pid",
            "--unshare-ipc",
            "--unshare-uts",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--perms",
            "0755",
            "--dir",
            "/etc",
            "--ro-bind",
            "/etc/alternatives",
            "/etc/alternatives",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        common.extend([
            "--ro-bind".into(),
            toolchain.into(),
            "/toolchain".into(),
            "--perms".into(),
            "0755".into(),
            "--dir".into(),
            "/cargo".into(),
            "--ro-bind".into(),
            registry.into(),
            "/cargo/registry".into(),
        ]);
        for (key, value) in [
            ("HOME", "/tmp"),
            ("CARGO_HOME", "/cargo"),
            ("PATH", "/toolchain/bin:/usr/bin"),
            ("TMPDIR", "/out/tmp"),
            ("LC_ALL", "C"),
            ("CARGO_NET_OFFLINE", "true"),
        ] {
            common.extend(["--setenv", key, value].map(OsString::from));
        }
        common.push("--bind".into());
        common.extend([
            work.join("out").into(),
            "/out".into(),
            "--chdir".into(),
            "/out".into(),
        ]);
        if Path::new("/lib64").is_dir() {
            common.extend(["--ro-bind", "/lib64", "/lib64"].map(Into::into));
        }
        let drop = if elevated {
            vec![
                "setpriv".into(),
                "--reuid".into(),
                uid.to_string().into(),
                "--regid".into(),
                gid.to_string().into(),
                "--clear-groups".into(),
                "--no-new-privs".into(),
                "--".into(),
            ]
        } else {
            vec![]
        };
        Ok(Self {
            elevated,
            common,
            drop,
        })
    }
    fn command(&self, options: &[&std::ffi::OsStr], args: &[&str]) -> Command {
        let mut c = if self.elevated {
            let mut c = Command::new("sudo");
            c.args(["-n", "bwrap"]);
            c
        } else {
            Command::new("bwrap")
        };
        c.args(&self.common)
            .args(options)
            .args(&self.drop)
            .args(args);
        c
    }
    fn check(&self, uid: u32) -> Result {
        let ns = fs::read_link("/proc/self/ns/net")?;
        require(
            stdout(&mut self.command(&[], &["readlink", "/proc/self/ns/net"]))?.trim()
                != ns.to_str().ok_or("invalid network namespace")?,
            "network namespace shared",
        )?;
        require(
            stdout(&mut self.command(&[], &["id", "-u"]))?.trim() == uid.to_string(),
            "sandbox uid differs",
        )?;
        run(&mut self.command(
            &[],
            &[
                "grep",
                "-Eq",
                "^CapEff:[[:space:]]+0+$",
                "/proc/self/status",
            ],
        ))?;
        run(&mut self.command(&[], &["test", "!", "-e", "/source"]))?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        connect(port)?;
        run(&mut self.command(
            &[
                "--ro-bind".as_ref(),
                env::current_exe()?.as_os_str(),
                "/probe".as_ref(),
            ],
            &["/probe", "probe-loopback", &port.to_string()],
        ))?;
        Ok(())
    }
}

fn passed(log: &str, count: usize) -> bool {
    let prefix = format!("test result: ok. {count} passed; 0 failed; 0 ignored;");
    log.lines().any(|line| line.starts_with(&prefix))
}

pub fn qualify(args: Args) -> Result {
    require(cfg!(target_os = "linux"), "native Linux host required")?;
    let uid = unsafe { libc::getuid() };
    require(uid != 0, "run as an unprivileged user")?;
    checkout(&args.original, ORIGINAL)?;
    checkout(&args.framework, FRAMEWORK)?;
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        rustc
            .lines()
            .any(|line| line == format!("commit-hash: {RUST}")),
        "unexpected compiler",
    )?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let out = work.join("out");
    fs::create_dir_all(out.join("tmp"))?;
    let source = work.join("source");
    fs::create_dir(&source)?;
    let target = work.join("framework-target");
    let mut cargo = Command::new("cargo");
    for name in [
        "RUSTC",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_BOOTSTRAP",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        cargo.env_remove(name);
    }
    run(cargo
        .current_dir(&args.framework)
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&target))?;
    let compiler = target.join("debug/oddlang");
    let inputs = work.join("in");
    super::stage(&args.portable, &inputs, None)?;
    let toolchain = PathBuf::from(
        stdout(Command::new("rustc").args(["+1.97.1", "--print", "sysroot"]))?.trim(),
    )
    .canonicalize()?;
    let cache = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|p| p.join(".cargo")))
        .ok_or("missing Cargo home")?
        .canonicalize()?;
    let sandbox = Sandbox::new(&work, &toolchain, &cache.join("registry"), uid, unsafe {
        libc::getgid()
    })?;
    sandbox.check(uid)?;
    run(&mut sandbox.command(
        &[
            "--ro-bind".as_ref(),
            compiler.as_os_str(),
            "/oddlang".as_ref(),
            "--ro-bind".as_ref(),
            inputs.as_os_str(),
            "/inputs".as_ref(),
        ],
        &[
            "/oddlang",
            "build",
            "native-package",
            "rust",
            "/inputs/core-package.json",
            "/out/generated",
        ],
    ))?;
    let package = out.join("generated");
    let lock = package.join("package.lock");
    require(
        fs::read(args.portable.join("core-package-v1/package.lock"))? == fs::read(&lock)?,
        "package lock differs",
    )?;
    let manifest = inputs.join("core-package.json");
    super::verify(&package, &manifest)?;
    let generated = ["--chdir".as_ref(), "/out/generated".as_ref()];
    run(&mut sandbox.command(
        &generated,
        &["cargo", "build", "--release", "--locked", "--offline"],
    ))?;
    let archive = work.join("source.tar");
    run(Command::new("git")
        .current_dir(&args.original)
        .args(["archive", ORIGINAL, "-o"])
        .arg(&archive))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&source))?;
    let tests = package.join("tests");
    fs::create_dir(&tests)?;
    let upstream = source.join("crates/marmish-core/tests");
    let contracts = tests.join("contracts.rs");
    fs::copy(upstream.join("contracts.rs"), &contracts)?;
    let caller = whole().join("core-package/caller.rs");
    for dir in [&tests, &upstream] {
        fs::copy(&caller, dir.join("differential.rs"))?;
    }
    let git_cache = cache.join("git");
    let original = [
        "--ro-bind".as_ref(),
        source.as_os_str(),
        "/source".as_ref(),
        "--ro-bind".as_ref(),
        git_cache.as_os_str(),
        "/cargo/git".as_ref(),
        "--chdir".as_ref(),
        "/source".as_ref(),
    ];
    for (name, options) in [("generated", &generated[..]), ("original", &original[..])] {
        let mut options = options.to_vec();
        let path = format!("/out/{name}.jsonl");
        options.extend_from_slice(&[
            "--setenv".as_ref(),
            "ODDLANG_CORE_TRANSCRIPT".as_ref(),
            path.as_ref(),
        ]);
        let log = stdout(&mut sandbox.command(
            &options,
            &[
                "cargo",
                "test",
                "--target-dir",
                &format!("/out/tests-{name}"),
                "--release",
                "--locked",
                "--offline",
                "-p",
                "marmish-core",
            ],
        ))?;
        require(
            passed(&log, 7) && passed(&log, 1),
            format!("missing successful test suites for {name}"),
        )?;
    }
    let transcript = out.join("original.jsonl");
    let cases: usize = super::compare(&transcript, &out.join("generated.jsonl"))?
        .values()
        .sum();
    let receipt = json!({
        "reference": ORIGINAL,
        "frameworkRevision": FRAMEWORK,
        "native": stdout(Command::new("uname").arg("-m"))?.trim(),
        "system": "Linux",
        "privilegeMode": if sandbox.elevated { "sudo-bwrap-setpriv" } else { "unprivileged-bwrap" },
        "effectiveCapabilities": 0,
        "isolatedHostLoopback": true,
        "verifiedGeneratedFileHashes": true,
        "rustc": rustc.lines().collect::<Vec<_>>(),
        "oddlangSha256": sha(&compiler)?,
        "packageLockSha256": sha(&lock)?,
        "graphManifestSha256": sha(&manifest)?,
        "callerSha256": sha(&caller)?,
        "contractsSha256": sha(&contracts)?,
        "publicCases": cases,
        "upstreamTestsPerBuild": 7,
        "transcriptSha256": sha(&transcript)?,
        "nativeGeneration": "bubblewrap; network and original source unavailable"
    });
    let bytes = json_bytes(&receipt)?;
    tmp.close()?;
    use std::io::Write;
    let parent = args
        .receipt
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.persist(args.receipt)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_probe_requires_connect_failure() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(probe_loopback(port).is_err());
        drop(listener);
        probe_loopback(port).unwrap();
    }
    #[test]
    fn complete_cargo_test_summary_required() {
        assert!(passed(
            "test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured",
            7
        ));
        assert!(!passed(
            "test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured",
            7
        ));
        assert!(!passed(
            "test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured",
            7
        ));
    }
}
