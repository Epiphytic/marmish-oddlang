//! Generate a native package from its staged graph inputs under macOS Seatbelt.
use crate::{Result, assembly::checkout, require, run, stdout};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use std::{
    env,
    ffi::OsString,
    fs,
    net::TcpListener,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

const GENERATION_PROFILE: &str = r#"(version 1)
(allow default)
(deny network*)
(deny file-read-data)
(deny file-write*)
(allow file-write* (subpath (param "stage")) (literal "/dev/null"))
(allow file-read-data
    (subpath (param "stage")) (subpath (param "sysroot"))
    (subpath (param "registry")) (subpath (param "sdk"))
    (subpath (param "developer")) (subpath (param "runtime"))
    (subpath "/System") (subpath "/usr") (subpath "/bin") (subpath "/sbin")
    (subpath "/opt/homebrew") (subpath "/dev") (subpath "/private/etc")
    (literal "/") (literal "/private") (literal "/tmp")
    (literal "/private/tmp") (literal "/private/var") (literal "/private/var/select/sh"))
(deny file-read-data
    (subpath (param "original")) (subpath (param "original-objects"))
    (subpath (param "companion")) (subpath (param "companion-objects"))
    (subpath (param "framework")) (subpath (param "oracle")))
"#;

fn path_output(cmd: &mut Command) -> Result<PathBuf> {
    Ok(PathBuf::from(stdout(cmd)?.trim_end()).canonicalize()?)
}
fn rust_tool(name: &str) -> Result<PathBuf> {
    path_output(Command::new("rustup").args(["which", "--toolchain", "1.97.1", name]))
}
fn objects(repo: &Path) -> Result<PathBuf> {
    path_output(Command::new("git").current_dir(repo).args([
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ]))
}
fn publish(src: &Path, dest: &Path) -> Result<()> {
    match renameat_with(CWD, src, CWD, dest, RenameFlags::NOREPLACE) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::XDEV) => {
            let temp = tempfile::tempdir_in(dest.parent().ok_or("missing output parent")?)?;
            let local = temp.path().join("workspace");
            run(Command::new("/bin/cp").args(["-pR"]).arg(src).arg(&local))?;
            renameat_with(CWD, &local, CWD, dest, RenameFlags::NOREPLACE)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

pub fn generate(
    framework: &Path,
    original: &Path,
    output: &Path,
    framework_pin: &str,
    project: &Path,
    transcript: &Path,
    oracle: Option<&Path>,
) -> Result<()> {
    let fw = framework.canonicalize()?;
    let original = original.canonicalize()?;
    checkout(&fw, framework_pin, false)?;
    let output = std::path::absolute(output)?;
    require(!output.try_exists()?, "output already exists")?;
    let project = project.canonicalize()?;
    let here = project.parent().ok_or("missing projector directory")?;
    let companion = path_output(
        Command::new("git")
            .current_dir(here)
            .args(["rev-parse", "--show-toplevel"]),
    )?;
    let oracle = oracle.map(fs::canonicalize).transpose()?;
    let temp = tempfile::Builder::new().prefix("oddlang-cli.").tempdir()?;
    let stage = temp.path().canonicalize()?;
    let cell = stage.join("cell");
    let home = stage.join("home");
    fs::create_dir(&cell)?;
    fs::create_dir_all(home.join(".cargo"))?;
    fs::create_dir(stage.join("tmp"))?;
    let rustc = rust_tool("rustc")?;
    let cargo = rust_tool("cargo")?;
    let version = stdout(Command::new(&rustc).arg("-vV"))?;
    require(
        version
            .lines()
            .any(|l| l == "commit-hash: 8bab26f4f68e0e26f0bb7960be334d5b520ea452"),
        "unexpected compiler revision",
    )?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let sysroot = path_output(Command::new(&rustc).args(["--print", "sysroot"]))?;
    let registry = match env::var_os("CARGO_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env::var_os("HOME").ok_or("missing HOME")?).join(".cargo"),
    }
    .join("registry")
    .canonicalize()?;
    let sdk = path_output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]))?;
    let developer = path_output(Command::new("xcode-select").arg("--print-path"))?;
    let xcrun = |name: &str| -> Result<String> {
        Ok(
            stdout(Command::new("xcrun").args(["--sdk", "macosx", "--find", name]))?
                .trim_end()
                .into(),
        )
    };
    let build_dir = tempfile::tempdir_in(&stage)?;
    let mut build = Command::new(&cargo);
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_BOOTSTRAP",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        build.env_remove(key);
    }
    run(build
        .current_dir(&fw)
        .env("RUSTC", &rustc)
        .args([
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(build_dir.path()))?;
    let compiler = stage.join("oddlang");
    fs::copy(build_dir.path().join("debug/oddlang"), &compiler)?;
    let (host_cmd, cell_cmd, runtime) = match project.extension().and_then(|p| p.to_str()) {
        Some("rs") => {
            let name = project.file_stem().ok_or("missing Rust projector name")?;
            run(Command::new("cargo")
                .args([
                    "+1.96.1",
                    "build",
                    "--quiet",
                    "--locked",
                    "--offline",
                    "--manifest-path",
                ])
                .arg(companion.join("verification/whole/canonical-public-data/Cargo.toml"))
                .arg("--bin")
                .arg(name)
                .arg("--target-dir")
                .arg(build_dir.path().join("tools")))?;
            let binary = cell.join(name);
            fs::copy(build_dir.path().join("tools/debug").join(name), &binary)?;
            (
                vec![binary.clone()],
                vec![binary, PathBuf::from("--root"), cell.clone()],
                sysroot.clone(),
            )
        }
        Some("py") => {
            let py = path_output(
                Command::new("python3").args(["-c", "import sys; print(sys.executable)"]),
            )?;
            let prefix =
                path_output(Command::new(&py).args(["-c", "import sys; print(sys.base_prefix)"]))?;
            (
                vec![py.clone(), project.clone()],
                vec![py, cell.join(project.strip_prefix(&companion)?)],
                prefix,
            )
        }
        _ => return Err("unknown projector type".into()),
    };
    run(Command::new(&host_cmd[0])
        .args(&host_cmd[1..])
        .arg("recover")
        .arg(&compiler)
        .arg(&original))?;
    run(Command::new(&host_cmd[0])
        .args(&host_cmd[1..])
        .arg("stage")
        .arg(&cell))?;
    build_dir.close()?;
    symlink(&registry, home.join(".cargo/registry"))?;
    let original_objects = objects(&original)?;
    let companion_objects = objects(&companion)?;
    let policy = stage.join("profile.sb");
    fs::write(&policy, GENERATION_PROFILE)?;
    let sandbox = || {
        let mut c = Command::new("sandbox-exec");
        for (name, path) in [
            ("stage", &stage),
            ("sysroot", &sysroot),
            ("registry", &registry),
            ("sdk", &sdk),
            ("developer", &developer),
            ("runtime", &runtime),
            ("original", &original),
            ("original-objects", &original_objects),
            ("companion", &companion),
            ("companion-objects", &companion_objects),
            ("framework", &fw),
            ("oracle", oracle.as_ref().unwrap_or(&original)),
        ] {
            let mut arg = OsString::from(name);
            arg.push("=");
            arg.push(path);
            c.arg("-D").arg(arg);
        }
        c.arg("-f").arg(&policy).current_dir(&stage);
        c
    };
    let mut probes = vec![
        original.join("crates/marmish-native/src/cli.rs"),
        transcript.canonicalize()?,
        original_objects.join("HEAD"),
        companion_objects.join("HEAD"),
        fw.join("src/main.rs"),
    ];
    if let Some(bins) = &oracle {
        probes.extend([bins.join("marmish"), bins.join("marmishd")]);
    }
    for path in probes {
        require(path.is_file(), "missing read-denial probe")?;
        let result = sandbox()
            .args(["/usr/bin/head", "-c", "1"])
            .arg(&path)
            .output()?;
        require(
            !result.status.success()
                && result.stdout.is_empty()
                && String::from_utf8_lossy(&result.stderr).contains("Operation not permitted"),
            format!("source isolation failed: {}", path.display()),
        )?;
    }
    let outside = tempfile::NamedTempFile::new_in("/private/tmp")?;
    let mut dest = OsString::from("of=");
    dest.push(outside.path());
    let write = sandbox()
        .args(["/bin/dd", "if=/dev/zero", "bs=1", "count=1"])
        .arg(dest)
        .output()?;
    require(
        !write.status.success()
            && outside.as_file().metadata()?.len() == 0
            && String::from_utf8_lossy(&write.stderr).contains("Operation not permitted"),
        "outside write probe failed",
    )?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port().to_string();
    run(Command::new("/usr/bin/nc").args(["-z", "-w", "1", "127.0.0.1", &port]))?;
    require(
        !sandbox()
            .args(["/usr/bin/nc", "-z", "-w", "1", "127.0.0.1", &port])
            .output()?
            .status
            .success(),
        "sandbox allowed loopback connection",
    )?;
    let clang = xcrun("clang")?;
    let ar = xcrun("ar")?;
    let path = env::join_paths([
        cargo.parent().ok_or("missing cargo directory")?,
        Path::new("/usr/bin"),
        Path::new("/bin"),
    ])?;
    let generated = stage.join("workspace");
    run(sandbox()
        .args(cell_cmd)
        .arg("generate")
        .arg(compiler)
        .arg(&generated)
        .current_dir(&cell)
        .env_clear()
        .env("PATH", path)
        .env("HOME", &home)
        .env("TMPDIR", stage.join("tmp"))
        .env("CARGO_HOME", home.join(".cargo"))
        .env("RUSTC", rustc)
        .env("CARGO_NET_OFFLINE", "true")
        .env("SDKROOT", sdk)
        .env("CC", &clang)
        .env("AR", ar)
        .env(
            format!(
                "CARGO_TARGET_{}_LINKER",
                host.to_uppercase().replace('-', "_")
            ),
            clang,
        ))?;
    publish(&generated, &output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_preserves_links_modes_and_existing_output() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let stage = tempfile::tempdir()?;
        let dest = tempfile::tempdir_in("/tmp")?;
        let source = stage.path().join("workspace");
        fs::create_dir(&source)?;
        fs::write(source.join("tool"), b"first")?;
        fs::set_permissions(source.join("tool"), fs::Permissions::from_mode(0o751))?;
        symlink("tool", source.join("link"))?;
        let output = dest.path().join("workspace");
        publish(&source, &output)?;
        assert_eq!(fs::read_link(output.join("link"))?, Path::new("tool"));
        assert_eq!(
            fs::metadata(output.join("tool"))?.permissions().mode() & 0o777,
            0o751
        );
        let other = stage.path().join("other");
        fs::create_dir(&other)?;
        assert!(publish(&other, &output).is_err());
        assert_eq!(fs::read(output.join("tool"))?, b"first");
        assert!(other.is_dir());
        Ok(())
    }
}
