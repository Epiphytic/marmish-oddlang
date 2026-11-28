//! Bootstrap the host build policy before Cargo evaluates any checkout or configuration.
#[path = "../canonical-public-data/src/build_host.rs"]
mod build_host;

use std::{
    env,
    ffi::OsStr,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
};

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [operation, path] = args.as_slice() else {
        return Err(io::Error::other(
            "expected check-config DIRECTORY or prepare CARGO_HOME",
        ));
    };
    let path = Path::new(path);
    match operation.to_str() {
        Some("check-config") => build_host::reject_local_cargo_config(path),
        Some("prepare") => {
            build_host::prepare(path)?;
            // Emit only the environment overrides. Bare names unset a variable.
            let cargo =
                build_host::cargo_command(path, Path::new("."), env::vars_os().map(|(key, _)| key));
            let mut out = io::stdout().lock();
            for (key, value) in cargo.get_envs() {
                out.write_all(key.as_bytes())?;
                if let Some(value) = value {
                    out.write_all(b"=")?;
                    out.write_all(OsStr::as_bytes(value))?;
                }
                out.write_all(b"\0")?;
            }
            Ok(())
        }
        _ => Err(io::Error::other("unknown build-host operation")),
    }
}
