//! Replay the same native 2024 stream caller against two independent release builds.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, bytes_sha},
    load, portable, require, run, sha, text, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env, fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};
use stream_wire_replay::{
    Environment, check_original, compare_caller, original_tree, write_receipt,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    oddlang: PathBuf,
    #[arg(long)]
    original: PathBuf,
    #[arg(long)]
    receipt: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
}
fn environment(work: &Path) -> Result<Environment> {
    let cache = if let Some(v) = env::var_os("CARGO_HOME") {
        PathBuf::from(v)
    } else {
        PathBuf::from(env::var_os("HOME").ok_or("missing HOME")?).join(".cargo")
    }
    .canonicalize()?;
    let isolated = work.join("cargo-home");
    fs::create_dir(&isolated)?;
    for name in ["registry", "git"] {
        if cache.join(name).is_dir() {
            symlink(cache.join(name), isolated.join(name))?;
        }
    }
    let mut out: Environment = env::vars_os()
        .filter(|(k, _)| {
            let k = k.to_string_lossy();
            !k.starts_with("CARGO_")
                && ![
                    "RUSTC",
                    "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER",
                    "RUSTFLAGS",
                    "RUSTDOC",
                    "RUSTDOCFLAGS",
                    "RUSTC_BOOTSTRAP",
                    "RUST_TARGET_PATH",
                ]
                .contains(&k.as_ref())
        })
        .collect();
    out.insert("CARGO_HOME".into(), isolated.into());
    out.insert("CARGO_NET_OFFLINE".into(), "true".into());
    out.insert("RUSTUP_TOOLCHAIN".into(), "1.97.1".into());
    Ok(out)
}
fn report(raw: &[u8]) -> Result<Vec<Value>> {
    let mut cases = Vec::new();
    let mut seen = BTreeSet::new();
    for line in std::str::from_utf8(raw)?.lines() {
        let mut case: Value = serde_json::from_str(line)?;
        require(
            seen.insert(text(&case["case"])?.to_owned()),
            "native caller repeated a case ID",
        )?;
        if let Some(encoded) = case["outcomes"]["encode"].as_str()
            && encoded.chars().count() > 1024
        {
            case["outcomes"]["encode"] =
                json!({"length":encoded.len(),"sha256":bytes_sha(encoded.as_bytes())});
        }
        cases.push(case);
    }
    Ok(cases)
}
fn main() -> Result {
    let a = Args::parse();
    require(
        a.framework_revision.len() == 40
            && a.framework_revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "expected a full framework revision",
    )?;
    let original = a.original.canonicalize()?;
    let rust = check_original(&original)?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path();
    let env = environment(work)?;
    let src = work.join("original");
    fs::create_dir(&src)?;
    let archive = work.join("original.tar");
    run(Command::new("git")
        .current_dir(&original)
        .args(["archive", "--output"])
        .arg(&archive)
        .arg(ORIGINAL))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&src))?;
    let manifest = a
        .manifest
        .unwrap_or_else(|| portable().join("stream-wire-package.json"))
        .canonicalize()?;
    let package = work.join("package");
    run(Command::new(&a.oddlang)
        .env_clear()
        .envs(&env)
        .args(["build", "native-package", "rust"])
        .arg(&manifest)
        .arg(&package))?;
    let lock = load(&package.join("package.lock"))?;
    require(
        lock["manifestSha256"] == sha(&manifest)?,
        "generated package lock uses a different manifest",
    )?;
    for (file, digest) in lock["generatedFiles"]
        .as_object()
        .ok_or("missing generated file list")?
    {
        require(
            digest == &sha(&package.join(file))?,
            format!("generated source differs: {file}"),
        )?;
    }
    let caller = whole().join("stream-wire-operations/caller.rs");
    let peer = |root: &Path, target: &str| {
        compare_caller(
            root,
            &work.join(target),
            work,
            &caller,
            &["marmish_stream", "serde_json"],
            "2024",
            Some(&env),
        )
    };
    let baseline = peer(&src, "original-cargo")?;
    let candidate = peer(&package, "candidate-cargo")?;
    if baseline != candidate {
        let original = report(&baseline)?;
        let generated = report(&candidate)?;
        let changed: BTreeSet<_> = original
            .iter()
            .chain(&generated)
            .filter(|r| !original.contains(r) || !generated.contains(r))
            .map(|r| r["case"].to_string())
            .collect();
        return Err(format!("native wire operation disagreement: {changed:?}").into());
    }
    let cases = report(&baseline)?;
    let count = cases.len();
    write_receipt(
        &a.receipt,
        json!({"format":"marmish-native-wire-operations-replay-v1","originalRevision":ORIGINAL,"originalTree":original_tree(&original)?,
        "originalCargoLockSha256":sha(&original.join("Cargo.lock"))?,"originalWireSourceSha256":sha(&src.join("crates/marmish-stream/src/wire.rs"))?,
        "rustc":rust,"frameworkRevision":a.framework_revision,"oddlangBinarySha256":sha(&a.oddlang)?,"packageManifestSha256":sha(&manifest)?,"packageLock":lock,
        "callerSha256":sha(&caller)?,"originalOutputSha256":bytes_sha(&baseline),"candidateOutputSha256":bytes_sha(&candidate),"cases":cases}),
    )?;
    println!("Matched {count} native wire case records");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_checks_ids_and_hashes_only_long_encoded_frames() -> Result {
        let short = "x".repeat(1024);
        let long = "y".repeat(1025);
        let a = json!({"case":"a","outcomes":{"encode":short}});
        let b = json!({"case":"b","outcomes":{"encode":long}});
        let rows = report(format!("{a}\n{b}\n").as_bytes())?;
        assert_eq!(rows[0], a);
        assert_eq!(
            rows[1]["outcomes"]["encode"],
            json!({"length":long.len(),"sha256":bytes_sha(long.as_bytes())})
        );
        assert!(report(format!("{a}\n{a}\n").as_bytes()).is_err());
        Ok(())
    }
}
