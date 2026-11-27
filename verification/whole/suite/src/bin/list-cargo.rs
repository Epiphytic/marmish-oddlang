//! Discover native release tests and ignored cases from the pinned upstream workspace.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use suite_receipts::UPSTREAM_PIN as PIN;
use suite_receipts::{Result, file_digest};

fn stdout(cmd: &mut Command) -> Result<String> {
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(format!(
            "{cmd:?} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?)
}
fn cases(listing: &str) -> Result<BTreeMap<&str, &str>> {
    let mut rows = BTreeMap::new();
    for line in listing.lines().filter(|l| !l.is_empty()) {
        let (name, kind) = line.rsplit_once(": ").ok_or("missing harness case kind")?;
        if !matches!(kind, "test" | "benchmark") || rows.insert(name, kind).is_some() {
            return Err(format!("unexpected harness output {line:?}").into());
        }
    }
    Ok(rows)
}
fn text(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| "expected string".into())
}
fn strings(v: &Value) -> Result<Vec<&str>> {
    v.as_array()
        .ok_or("expected array")?
        .iter()
        .map(text)
        .collect()
}
fn inventory(source: &Path, target: &Path) -> Result<Value> {
    let git = |args: &[&str]| stdout(Command::new("git").current_dir(source).args(args));
    if git(&["rev-parse", "HEAD"])?.trim() != PIN || !git(&["status", "--porcelain"])?.is_empty() {
        return Err("expected a clean upstream reference checkout".into());
    }
    let cargo = |args: &[&str]| {
        let mut c = Command::new("cargo");
        c.current_dir(source).arg("+1.97.1").args(args);
        c
    };
    let metadata: Value = serde_json::from_str(&stdout(&mut cargo(&[
        "metadata",
        "--format-version=1",
        "--locked",
        "--offline",
        "--no-deps",
    ]))?)?;
    let members: BTreeSet<_> = strings(&metadata["workspace_members"])?
        .into_iter()
        .collect();
    let mut packages = BTreeMap::new();
    for p in metadata["packages"].as_array().ok_or("missing packages")? {
        let id = text(&p["id"])?;
        if members.contains(id) {
            packages.insert(id, p);
        }
    }
    let build = stdout(
        cargo(&[
            "test",
            "--workspace",
            "--release",
            "--locked",
            "--offline",
            "--no-run",
            "--message-format=json",
            "--target-dir",
        ])
        .arg(target),
    )?;
    let mut harnesses = BTreeMap::new();
    let mut any_tests = false;
    for line in build.lines() {
        let event: Value = serde_json::from_str(line)?;
        if event["reason"] != "compiler-artifact"
            || event["profile"]["test"] != true
            || event["executable"].is_null()
        {
            continue;
        }
        let p = packages
            .get(text(&event["package_id"])?)
            .ok_or("unexpected package")?;
        let entry = &event["target"];
        let key = format!(
            "{}/{}/{}",
            text(&p["name"])?,
            strings(&entry["kind"])?.join("+"),
            text(&entry["name"])?
        );
        let exe = Path::new(text(&event["executable"])?);
        let cwd = Path::new(text(&p["manifest_path"])?)
            .parent()
            .ok_or("missing manifest parent")?;
        let listing = |args: &[&str]| {
            stdout(
                Command::new(exe)
                    .current_dir(cwd)
                    .args(["--list", "--format", "terse"])
                    .args(args),
            )
        };
        let all = listing(&[])?;
        let skipped = listing(&["--ignored"])?;
        let names = cases(&all)?;
        let ignored = cases(&skipped)?;
        if !ignored.iter().all(|(n, k)| names.get(n) == Some(k)) {
            return Err(format!("ignored case absent from full list in {key}").into());
        }
        let tests: Vec<_> = names
            .iter()
            .filter(|(n, k)| **k == "test" && !ignored.contains_key(*n))
            .map(|(n, _)| n)
            .collect();
        any_tests |= !tests.is_empty();
        let mut features = strings(&event["features"])?;
        features.sort();
        let row = json!({"source":Path::new(text(&entry["src_path"])?).strip_prefix(source)?,
            "binarySha256":file_digest(exe)?,"features":features,"tests":tests,"ignored":ignored.keys().collect::<Vec<_>>(),
            "benchmarks":names.iter().filter(|(_,k)|**k=="benchmark").map(|(n,_)|n).collect::<Vec<_>>()});
        if harnesses.insert(key, row).is_some() {
            return Err("ambiguous test harness".into());
        }
    }
    if !any_tests {
        return Err("build yielded no workspace tests".into());
    }
    Ok(
        json!({"upstream":PIN,"cargoLockSha256":file_digest(&source.join("Cargo.lock"))?,
        "rustc":stdout(Command::new("rustc").current_dir(source).args(["+1.97.1","-vV"]))?.trim(),
        "selection":"native release workspace, default features","harnesses":harnesses}),
    )
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [source, target, out] = args.as_slice() else {
        return Err("usage: list-cargo UPSTREAM TARGET_DIR OUTPUT".into());
    };
    let source = Path::new(source).canonicalize()?;
    let target = std::path::absolute(PathBuf::from(target))?;
    let report = inventory(&source, &target)?;
    fs::write(out, format!("{}\n", serde_json::to_string_pretty(&report)?))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_libtest_names() {
        let rows = cases("module::case: test\ncontains: separator: benchmark\n").unwrap();
        assert_eq!(rows["module::case"], "test");
        assert_eq!(rows["contains: separator"], "benchmark");
        for line in [
            "duplicate: test\nduplicate: test",
            "missing",
            "test: unexpected",
        ] {
            assert!(cases(line).is_err());
        }
    }
}
