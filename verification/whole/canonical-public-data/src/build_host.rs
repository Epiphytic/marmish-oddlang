//! Cargo environment shared by host builds of the upstream oracle and compiler.
use std::{
    env,
    ffi::OsString,
    fs, io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

pub const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
pub fn private_home(home: &Path, cache: &Path) -> std::io::Result<()> {
    fs::create_dir(home)?;
    for name in ["registry", "git"] {
        let src = cache.join(name);
        if src.is_dir() {
            symlink(src.canonicalize()?, home.join(name))?;
        }
    }
    Ok(())
}

pub fn cargo_command(home: &Path, dir: &Path, keys: impl Iterator<Item = OsString>) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(dir);
    for key in keys {
        if ["CARGO_", "RUSTC", "RUSTDOC", "RUSTFLAGS"]
            .iter()
            .any(|p| key.as_encoded_bytes().starts_with(p.as_bytes()))
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("RUSTUP_TOOLCHAIN", "1.97.1")
        .env("MARMISH_BUILD_COMMIT", ORIGINAL)
        .env("CARGO_HOME", home)
        .env("CARGO_NET_OFFLINE", "true");
    cmd
}

pub fn prepare(home: &Path) -> io::Result<()> {
    let cache = match env::var_os("CARGO_HOME") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(env::var_os("HOME").ok_or_else(|| io::Error::other("missing HOME"))?)
            .join(".cargo"),
    };
    private_home(home, &cache)
}

pub fn reject_local_cargo_config(path: &Path) -> io::Result<()> {
    for dir in path.ancestors() {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let config = dir.join(name);
            if config.is_file() {
                return Err(io::Error::other(format!(
                    "unexpected Cargo configuration: {}",
                    config.display()
                )));
            }
        }
    }
    Ok(())
}
