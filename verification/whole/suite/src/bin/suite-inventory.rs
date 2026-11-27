//! Inventory the test entry points in pinned upstream Git objects.
use regex::bytes::Regex;
use serde_json::{Value, json};
use std::{collections::BTreeSet, env, path::Path, process::Command, str};
use suite_receipts::{Result, UPSTREAM_PIN};

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git").current_dir(repo).args(args).output()?;
    if !out.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(out.stdout)
}
fn inventory(repo: &Path) -> Result<Value> {
    if str::from_utf8(&git(repo, &["rev-parse", "HEAD"])?)?.trim() != UPSTREAM_PIN {
        return Err(format!("expected upstream HEAD {UPSTREAM_PIN}").into());
    }
    let tree = String::from_utf8(git(
        repo,
        &["rev-parse", &format!("{UPSTREAM_PIN}^{{tree}}")],
    )?)?;
    let mut result = json!({"upstream_commit":UPSTREAM_PIN,"upstream_tree":tree.trim(),
        "rust_unit_source_files":[],"rust_integration_targets":[],"rust_integration_support_files":[],
        "python_test_modules":[],"node_test_modules":[],"c_fixture_test_sources":[],"workflow_files":[]});
    let bin_env = Regex::new(r#"(?:option_)?env!\(\s*"CARGO_BIN_EXE_([^"]+)""#)?;
    let rust_test = Regex::new(r"(?m)^\s*#\[\s*(?:(?:tokio|async_std)::)?test\b")?;
    for entry in git(repo, &["ls-tree", "-r", "-z", UPSTREAM_PIN])?
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
    {
        let (header, path) = str::from_utf8(entry)?
            .split_once('\t')
            .ok_or("invalid Git tree entry")?;
        let blob = header.split_whitespace().nth(2).ok_or("missing blob id")?;
        let mut row = json!({"path":path,"git_blob":blob});
        let parts: Vec<_> = path.split('/').collect();
        let name = parts.last().ok_or("empty path")?;
        let group = if path.starts_with(".github/workflows/") {
            "workflow_files"
        } else if name.ends_with(".test.mjs") {
            "node_test_modules"
        } else if name.starts_with("test_") && name.ends_with(".py") {
            "python_test_modules"
        } else if name.ends_with("-test.c") {
            "c_fixture_test_sources"
        } else if name.ends_with(".rs") && matches!(parts[0], "crates" | "tools") {
            let source = git(repo, &["cat-file", "blob", blob])?;
            let mut refs = BTreeSet::new();
            for cap in bin_env.captures_iter(&source) {
                refs.insert(str::from_utf8(cap.get(1).unwrap().as_bytes())?);
            }
            if !refs.is_empty() {
                row["compile_time_executables"] = json!(refs);
            }
            if parts.get(2) == Some(&"tests") {
                if parts.len() == 4 {
                    "rust_integration_targets"
                } else {
                    "rust_integration_support_files"
                }
            } else if rust_test.is_match(&source) {
                "rust_unit_source_files"
            } else {
                continue;
            }
        } else {
            continue;
        };
        result[group]
            .as_array_mut()
            .expect("known inventory group")
            .push(row);
    }
    Ok(result)
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [source] = args.as_slice() else {
        return Err("usage: suite-inventory UPSTREAM".into());
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&inventory(Path::new(source))?)?
    );
    Ok(())
}
