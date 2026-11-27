//! Stage the locked portable closure and run the native builder with first-party sources denied.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};
pub const DARWIN: &str = "aarch64-apple-darwin";
pub const LINUX: &str = "aarch64-unknown-linux-gnu";
pub const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
pub const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
pub fn root() -> &'static Path {
    whole().ancestors().nth(2).unwrap()
}
pub fn checkout(path: &Path, revision: &str, reject_config: bool) -> Result {
    if reject_config {
        for p in path.ancestors() {
            for name in [".cargo/config", ".cargo/config.toml"] {
                require(
                    !p.join(name).is_file(),
                    format!("unexpected Cargo configuration in {}", p.display()),
                )?;
            }
        }
    }
    require(
        stdout(
            Command::new("git")
                .current_dir(path)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == revision
            && stdout(Command::new("git").current_dir(path).args([
                "status",
                "--porcelain",
                "--untracked-files=all",
            ]))?
            .is_empty(),
        format!(
            "expected a clean checkout at {revision}: {}",
            path.display()
        ),
    )
}
pub fn build_env(cmd: &mut Command) -> &mut Command {
    cmd.env("RUSTUP_TOOLCHAIN", "1.97.1")
        .env("CARGO_NET_OFFLINE", "true")
        .env("MARMISH_BUILD_COMMIT", ORIGINAL);
    for k in [
        "CARGO_BUILD_TARGET",
        "CARGO_TARGET_DIR",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        cmd.env_remove(k);
    }
    cmd
}
pub fn pinned_copy(base: &Path, pin: &Value, out: &Path) -> Result<PathBuf> {
    let path = base.join(text(&pin["path"])?).canonicalize()?;
    let root = portable().canonicalize()?;
    let relative = path.strip_prefix(root)?;
    let bytes = fs::read(&path)?;
    require(pin["sha256"] == bytes_sha(&bytes), "invalid closure input")?;
    let dest = out.join(relative);
    fs::create_dir_all(dest.parent().ok_or("missing input parent")?)?;
    fs::write(dest, bytes)?;
    Ok(path)
}
pub fn stage(manifest: &Path, out: &Path) -> Result<Value> {
    fs::create_dir_all(out)?;
    let data = fs::read(manifest)?;
    let spec: Value = serde_json::from_slice(&data)?;
    fs::write(
        out.join(manifest.file_name().ok_or("missing manifest name")?),
        data,
    )?;
    pinned_copy(&portable(), &spec["cargoLock"], out)?;
    for m in spec["members"]
        .as_object()
        .ok_or("expected workspace members")?
        .values()
    {
        let source = pinned_copy(&portable(), &m["manifest"], out)?;
        let staged_member = out.join(source.strip_prefix(portable().canonicalize()?)?);
        let member = load(&staged_member)?;
        let base = source.parent().ok_or("missing member parent")?;
        pinned_copy(base, &member["cargoLock"], out)?;
        for component in list(&member["components"])? {
            for key in ["graph", "overlay"] {
                pinned_copy(base, &component[key], out)?;
            }
            if let Some(shape) = component.get("wireShape") {
                for value in shape.as_object().ok_or("invalid wire shape")?.values() {
                    pinned_copy(base, value, out)?;
                }
            }
        }
    }
    Ok(spec)
}
pub fn bytes_sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
pub fn inventory(root: &Path) -> Result<Value> {
    fn visit(base: &Path, dir: &Path, rows: &mut BTreeMap<PathBuf, String>) -> Result {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let path = e.path();
            if e.file_type()?.is_dir() {
                visit(base, &path, rows)?;
            } else {
                require(
                    e.file_type()?.is_file()
                        && ["json", "ttl", "lock"].contains(
                            &path.extension().and_then(OsStr::to_str).unwrap_or_default(),
                        ),
                    "unexpected non-portable closure input",
                )?;
                rows.insert(path.strip_prefix(base)?.to_owned(), sha(&path)?);
            }
        }
        Ok(())
    }
    let mut rows = BTreeMap::new();
    visit(root, root, &mut rows)?;
    Ok(json!(rows))
}
fn cargo_home() -> Result<PathBuf> {
    Ok(env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or(PathBuf::from(env::var_os("HOME").ok_or("missing HOME")?).join(".cargo")))
}
pub fn providers(lock: &Path, out: &Path) -> Result<Value> {
    let keys = BTreeMap::from([
        (
            "https://github.com/erskingardner/openmls.git",
            "openmls-daf8a12ad11cf9dc",
        ),
        (
            "https://github.com/erskingardner/rust-nostr",
            "rust-nostr-86bd2a4a0ba8c210",
        ),
        (
            "https://github.com/marmot-protocol/mdk",
            "mdk-7d5a3a2420b194f5",
        ),
        (
            "https://github.com/rusqlite/rusqlite.git",
            "rusqlite-8e5e8204e489f2fd",
        ),
    ]);
    let lock: toml::Table = toml::from_str(&fs::read_to_string(lock)?)?;
    let mut revisions = BTreeMap::new();
    let mut provider_receipt = json!({});
    for pkg in lock["package"]
        .as_array()
        .ok_or("missing locked packages")?
    {
        let Some(source) = pkg
            .get("source")
            .and_then(|s| s.as_str())
            .and_then(|s| s.strip_prefix("git+"))
        else {
            continue;
        };
        let (repo, pin) = source.split_once("?rev=").ok_or("expected exact Git rev")?;
        let (rev, fragment) = pin.split_once('#').ok_or("missing locked Git revision")?;
        provider_receipt[repo] = rev.into();
        require(
            rev == fragment
                && rev.len() == 40
                && rev
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                && revisions.insert(repo, rev).is_none_or(|old| old == rev),
            "invalid or conflicting Git provider revision",
        )?;
    }
    require(
        revisions.keys().eq(keys.keys()),
        "unexpected Git provider inventory",
    )?;
    let db = out.join("db");
    fs::create_dir_all(&db)?;
    for (url, rev) in &revisions {
        let key = keys[url];
        let from = cargo_home()?.join("git/db").join(key).canonicalize()?;
        let dst = db.join(key);
        run(Command::new("git")
            .args(["init", "--quiet", "--bare"])
            .arg(&dst))?;
        run(Command::new("git")
            .current_dir(&dst)
            .args(["fetch", "--quiet", "--depth=1", "--no-tags"])
            .arg(&from)
            .arg(rev))?;
        require(
            stdout(
                Command::new("git")
                    .current_dir(&dst)
                    .args(["rev-parse", "FETCH_HEAD"]),
            )?
            .trim()
                == *rev
                && fs::read_to_string(dst.join("shallow"))?.trim() == *rev,
            "Git provider did not stage exactly its locked commit",
        )?;
    }
    Ok(provider_receipt)
}
pub struct Sandbox {
    prefix: Vec<String>,
    env: BTreeMap<String, String>,
    work: PathBuf,
    linux: bool,
}
fn command_text(tool: &str, args: &[&str]) -> Result<String> {
    Ok(stdout(Command::new(tool).args(args))?.trim().into())
}
fn string(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("non-UTF-8 path")?.into())
}
impl Sandbox {
    pub fn new(host: &str, work: &Path, denied: &[&Path], providers: &Path) -> Result<Self> {
        let toolchain = PathBuf::from(command_text("rustc", &["+1.97.1", "--print", "sysroot"])?)
            .canonicalize()?;
        let registry = cargo_home()?.join("registry").canonicalize()?;
        if host == LINUX {
            let probe = capture(
                Command::new("bwrap").args([
                    "--unshare-all",
                    "--ro-bind",
                    "/",
                    "/",
                    "/usr/bin/true",
                ]),
                600,
                None,
            )?;
            let elevated = !probe.status.success();
            let mut prefix: Vec<String> = if elevated {
                vec!["sudo".into(), "-n".into(), "bwrap".into()]
            } else {
                vec!["bwrap".into()]
            };
            let ws = string(work)?;
            let tool = string(&toolchain)?;
            let reg = string(&registry)?;
            let cache = string(providers)?;
            prefix.extend(
                [
                    "--unshare-net",
                    "--unshare-ipc",
                    "--unshare-uts",
                    "--unshare-pid",
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
                    "--perms",
                    "0755",
                    "--dir",
                    "/etc",
                    "--ro-bind",
                    "/etc/alternatives",
                    "/etc/alternatives",
                    "--tmpfs",
                    "/tmp",
                    "--ro-bind",
                    &tool,
                    "/toolchain",
                    "--perms",
                    "0755",
                    "--dir",
                    "/cargo",
                    "--ro-bind",
                    &reg,
                    "/cargo/registry",
                    "--bind",
                    &ws,
                    &ws,
                    "--chdir",
                    &ws,
                    "--setenv",
                    "PATH",
                    "/toolchain/bin:/usr/bin",
                    "--setenv",
                    "CARGO_HOME",
                    "/cargo",
                    "--setenv",
                    "HOME",
                    &ws,
                    "--setenv",
                    "TMPDIR",
                    &ws,
                    "--setenv",
                    "CARGO_NET_OFFLINE",
                    "true",
                    "--setenv",
                    "LC_ALL",
                    "C",
                    "--bind",
                    &cache,
                    "/cargo/git",
                ]
                .map(str::to_owned),
            );
            if Path::new("/lib64").exists() {
                prefix.extend(["--ro-bind", "/lib64", "/lib64"].map(str::to_owned));
            }
            if elevated {
                prefix.extend([
                    "setpriv".into(),
                    "--reuid".into(),
                    command_text("id", &["-u"])?,
                    "--regid".into(),
                    command_text("id", &["-g"])?,
                    "--clear-groups".into(),
                    "--no-new-privs".into(),
                    "--".into(),
                ]);
            }
            let sandbox = Self {
                prefix,
                env: env::vars().collect(),
                work: work.into(),
                linux: true,
            };
            require(
                stdout(
                    sandbox
                        .command(work)?
                        .args(["readlink", "/proc/self/ns/net"]),
                )?
                .trim()
                    != string(&fs::read_link("/proc/self/ns/net")?)?,
                "network namespace was shared",
            )?;
            run(sandbox.command(work)?.args([
                "grep",
                "-Eq",
                "^CapEff:[[:space:]]+0+$",
                "/proc/self/status",
            ]))?;
            return Ok(sandbox);
        }
        require(host == DARWIN, "unsupported native host")?;
        let developer = PathBuf::from(command_text("xcode-select", &["-p"])?).canonicalize()?;
        let sdk = PathBuf::from(command_text(
            "xcrun",
            &["--sdk", "macosx", "--show-sdk-path"],
        )?)
        .canonicalize()?;
        let find = |name: &str| command_text("xcrun", &["--sdk", "macosx", "--find", name]);
        let clang = PathBuf::from(find("clang")?).canonicalize()?;
        let make = PathBuf::from(find("make")?);
        let app = developer
            .ancestors()
            .find(|p| p.extension() == Some(OsStr::new("app")))
            .ok_or("expected Xcode app directory")?;
        let home = work.join("candidate-cargo");
        fs::create_dir(&home)?;
        symlink(&registry, home.join("registry"))?;
        symlink(providers, home.join("git"))?;
        let tmp = work.join("tmp");
        fs::create_dir(&tmp)?;
        let policy_paths = |paths: Vec<&Path>| -> Result<String> {
            Ok(paths
                .iter()
                .map(|p| Ok(format!("(subpath {})", serde_json::to_string(&string(p)?)?)))
                .collect::<Result<Vec<_>>>()?
                .join(" "))
        };
        let mut reads: Vec<_> = [
            "/System",
            "/usr",
            "/bin",
            "/sbin",
            "/Library/Developer",
            "/private/etc/ssl",
        ]
        .iter()
        .map(Path::new)
        .collect();
        reads.extend([
            &toolchain,
            &developer,
            &registry,
            &sdk,
            clang.parent().ok_or("missing clang directory")?,
            app,
            work,
        ]);
        let policy = format!(
            "(version 1) (deny default) (allow process* sysctl-read mach-lookup file-read-metadata) (allow file-read* {} (literal \"/\") (literal \"/dev/null\") (literal \"/dev/random\") (literal \"/dev/urandom\") (literal \"/private/etc/localtime\")) (allow file-write* (subpath {}) (literal \"/dev/null\")) (deny file-read* {}) (deny network*)",
            policy_paths(reads)?,
            serde_json::to_string(&string(work)?)?,
            policy_paths(denied.to_vec())?
        );
        let env = BTreeMap::from([
            (
                "PATH",
                format!(
                    "{}:{}/bin:/usr/bin:/bin",
                    make.parent().ok_or("missing make directory")?.display(),
                    toolchain.display()
                ),
            ),
            ("HOME", string(work)?),
            ("CARGO_HOME", string(&home)?),
            ("RUSTC", string(&toolchain.join("bin/rustc"))?),
            ("RUSTDOC", string(&toolchain.join("bin/rustdoc"))?),
            ("TMPDIR", string(&tmp)?),
            ("CARGO_NET_OFFLINE", "true".into()),
            ("LC_ALL", "C".into()),
            ("DEVELOPER_DIR", string(&developer)?),
            ("SDKROOT", string(&sdk)?),
            ("CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER", string(&clang)?),
            ("CC", string(&clang)?),
            ("AR", find("ar")?),
            ("RANLIB", find("ranlib")?),
            ("MACOSX_DEPLOYMENT_TARGET", "11.0".into()),
            ("XCRUN_CACHE_DISABLED", "1".into()),
        ])
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        Ok(Self {
            prefix: vec!["/usr/bin/sandbox-exec".into(), "-p".into(), policy],
            env,
            work: work.into(),
            linux: false,
        })
    }
    pub fn command(&self, cwd: &Path) -> Result<Command> {
        let mut prefix = self.prefix.clone();
        if self.linux {
            let i = prefix
                .iter()
                .position(|s| s == "--chdir")
                .ok_or("missing sandbox work directory")?;
            prefix[i + 1] = string(cwd)?;
        }
        let mut cmd = Command::new(&prefix[0]);
        cmd.args(&prefix[1..])
            .current_dir(cwd)
            .env_clear()
            .envs(&self.env);
        Ok(cmd)
    }
    pub fn deny(&self, paths: &[PathBuf]) -> Result {
        for p in paths {
            require(
                p.is_file(),
                format!("missing source-denial probe: {}", p.display()),
            )?;
            let r = capture(self.command(&self.work)?.arg("/bin/cat").arg(p), 600, None)?;
            let err = String::from_utf8_lossy(&r.stderr);
            require(
                r.status.code() == Some(1)
                    && r.stdout.is_empty()
                    && (err.contains("Operation not permitted")
                        || (self.linux && err.contains("No such file"))),
                format!("source isolation failed to deny {}", p.display()),
            )?;
        }
        Ok(())
    }
}

pub fn archive(repo: &Path, revision: &str, dest: &Path) -> Result {
    let tar = tempfile::NamedTempFile::new()?;
    run(Command::new("git")
        .current_dir(repo)
        .args(["archive", "--output"])
        .arg(tar.path())
        .arg(revision))?;
    fs::create_dir(dest)?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(tar.path())
        .arg("-C")
        .arg(dest))?;
    Ok(())
}
