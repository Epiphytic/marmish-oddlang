//! Run the independent native and generated callers for the gap data contracts.
use crate::{Result, capture, require, sha, text, whole};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
};
const COPY: &str = "fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }";
pub fn command(tool: impl AsRef<std::ffi::OsStr>, dir: &Path) -> Command {
    let mut c = Command::new(tool);
    c.current_dir(dir).env_clear();
    for key in [
        "HOME",
        "PATH",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "SDKROOT",
    ] {
        if let Some(v) = env::var_os(key) {
            c.env(key, v);
        }
    }
    c
}
pub fn checked(c: &mut Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
    let out = capture(c, 1800, input)?;
    require(
        out.status.success(),
        format!(
            "{c:?}: {} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ),
    )?;
    Ok(out.stdout)
}
fn rows(bytes: &[u8]) -> Result<Vec<Value>> {
    Ok(std::str::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?)
}
fn array(v: &Value) -> Result<&Vec<Value>> {
    Ok(v.as_array().ok_or("expected array")?)
}
fn errors(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .filter(|v| v["level"] == "error")
        .filter_map(|v| v["code"]["code"].as_str().map(str::to_owned))
        .collect()
}

pub fn reference(
    original: &Path,
    target: &Path,
    host: &str,
    probe: &Path,
    rustc_args: &[&str],
    args: &[&str],
    vectors: &[u8],
) -> Result<(Value, Value, Value)> {
    let build = checked(
        command("cargo", original)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--lib",
                "-p",
                "marmish-engine",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(target)
            .arg("--message-format=json"),
        None,
    )?;
    let artifacts = rows(&build)?;
    let mut libs = BTreeMap::new();
    let mut links = BTreeSet::new();
    for r in &artifacts {
        if r["reason"] == "build-script-executed" {
            for l in array(&r["linked_paths"])? {
                links.insert(text(l)?.to_owned());
            }
        }
        if r["reason"] != "compiler-artifact" || r["target"]["kind"] != json!(["lib"]) {
            continue;
        }
        let name = text(&r["target"]["name"])?;
        if !["marmish_engine", "serde_json"].contains(&name) {
            continue;
        }
        let paths: Vec<_> = array(&r["filenames"])?
            .iter()
            .map(text)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(Path::new)
            .filter(|p| p.extension().is_some_and(|e| e == "rlib"))
            .collect();
        if !paths.iter().any(|p| p.starts_with(target.join(host))) {
            continue;
        }
        let [lib] = paths.as_slice() else {
            return Err("ambiguous library artifact".into());
        };
        if name == "marmish_engine" {
            require(
                Path::new(text(&r["manifest_path"])?).canonicalize()?
                    == original.join("crates/marmish-engine/Cargo.toml"),
                "unexpected engine package",
            )?;
        }
        require(
            libs.insert(name, lib.to_owned()).is_none(),
            "duplicate library artifact",
        )?;
    }
    require(libs.len() == 2, "missing selected libraries")?;
    let seal = || -> Result<BTreeMap<&str, String>> {
        libs.iter()
            .map(|(name, path)| Ok((*name, sha(path)?)))
            .collect()
    };
    let hashes = seal()?;
    let scratch = tempfile::tempdir()?;
    let work = scratch.path();
    let rustc = || {
        let mut c = command("rustc", work);
        c.args(["+1.97.1", "--edition=2024", "--target", host]);
        for (name, lib) in &libs {
            c.arg("--extern").arg(format!("{name}={}", lib.display()));
        }
        for path in [
            target.join(host).join("release/deps"),
            target.join("release/deps"),
        ] {
            c.arg("-L").arg(format!("dependency={}", path.display()));
        }
        for path in &links {
            c.arg("-L").arg(path);
        }
        c
    };
    let exe = work.join("probe");
    checked(
        rustc().args(rustc_args).arg(probe).arg("-o").arg(&exe),
        None,
    )?;
    let control = work.join("copy_control.rs");
    fs::write(&control, COPY)?;
    let result = capture(
        rustc()
            .args(["--error-format=json", "--emit=metadata"])
            .arg(control)
            .arg("-o")
            .arg(work.join("control.rmeta")),
        120,
        None,
    )?;
    let codes = errors(&rows(&result.stderr)?);
    require(
        !result.status.success() && codes == ["E0277"],
        "unexpected Copy control",
    )?;
    let observed: Value = serde_json::from_slice(&checked(
        command(exe.to_str().ok_or("UTF-8 probe path")?, work).args(args),
        Some(vectors),
    )?)?;
    require(
        hashes == seal()?,
        "selected release library changed during probe",
    )?;
    Ok((observed, json!(codes), json!(hashes)))
}

pub fn candidate(
    work: &Path,
    host: &str,
    probe: &Path,
    args: &[&str],
    vectors: &[u8],
) -> Result<(Value, Value)> {
    let lane = whole().join("gap-range-reference");
    let caller = work.join("caller");
    fs::create_dir_all(caller.join("src"))?;
    for (src, dst) in [("caller.toml", "Cargo.toml"), ("caller.lock", "Cargo.lock")] {
        fs::copy(lane.join(src), caller.join(dst))?;
    }
    fs::copy(probe, caller.join("src/main.rs"))?;
    let cargo = |verb: &str| {
        let mut cmd = command("cargo", &caller);
        cmd.args([
            "+1.97.1",
            verb,
            "--offline",
            "--locked",
            "--release",
            "--target",
            host,
        ]);
        cmd
    };
    let observations: Value = serde_json::from_slice(&checked(
        cargo("run").arg("--quiet").arg("--").args(args),
        Some(vectors),
    )?)?;
    fs::write(caller.join("src/main.rs"), COPY)?;
    let result = capture(cargo("check").arg("--message-format=json"), 120, None)?;
    let messages: Vec<_> = rows(&result.stdout)?
        .into_iter()
        .filter(|r| r["reason"] == "compiler-message")
        .map(|r| r["message"].clone())
        .collect();
    let codes = errors(&messages);
    require(
        !result.status.success() && codes == ["E0277"],
        "unexpected Copy diagnostic",
    )?;
    Ok((observations, json!(codes)))
}
