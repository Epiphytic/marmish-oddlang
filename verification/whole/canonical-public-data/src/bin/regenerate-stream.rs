//! Materialize the five sealed stream sources or verify an existing tree.
use public_data_qualification::{Result, assembly, require};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use std::{collections::BTreeSet, env, ffi::OsString, fs, path::Path, process::Command};

fn names(dir: &Path) -> Result<BTreeSet<OsString>> {
    Ok(fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<std::io::Result<_>>()?)
}

fn compare(expected: &Path, actual: &Path) -> Result<()> {
    let kind = fs::symlink_metadata(expected)?.file_type();
    let other = fs::symlink_metadata(actual)?.file_type();
    let same = if kind.is_dir() && other.is_dir() {
        let children = names(expected)?;
        if children != names(actual)? {
            false
        } else {
            for name in children {
                compare(&expected.join(&name), &actual.join(&name))?;
            }
            true
        }
    } else {
        kind.is_file() && other.is_file() && fs::read(expected)? == fs::read(actual)?
    };
    require(
        same,
        format!("generated source differs at {}", actual.display()),
    )
}

fn main() -> Result<()> {
    let root = assembly::root();
    let package = root.join("crates/marmish-stream");
    let stage = tempfile::Builder::new()
        .prefix(".oddlang-stream.")
        .tempdir_in(&package)?;
    let generated = stage.path().join("generated");
    let pin = fs::read_to_string(root.join("oddlang/stream.sha256"))?;
    let compiler = env::var_os("ODDLANG")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "oddlang".into());
    let status = Command::new(compiler)
        .args(["build", "rust-tree"])
        .arg(root.join("oddlang/stream"))
        .arg(&generated)
        .arg("--bundle-sha256")
        .arg(pin.trim_end_matches('\n'))
        .status()?;
    require(
        status.success(),
        format!("stream generation failed ({status})"),
    )?;
    let source = generated.join("src");
    require(
        fs::symlink_metadata(&source)?.is_dir(),
        "expected generated source directory",
    )?;
    let dest = package.join("src");
    match renameat_with(CWD, &source, CWD, &dest, RenameFlags::NOREPLACE) {
        Ok(()) => {}
        Err(rustix::io::Errno::EXIST) => compare(&source, &dest)?,
        Err(error) => return Err(error.into()),
    }
    println!("Verified generated Rust in {}", dest.display());
    Ok(())
}
