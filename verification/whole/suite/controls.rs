//! Independent fresh-process test observations and coverage controls.
use public_data_qualification::{Result, capture, portable, require, run, sha, stdout};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Command, ExitStatus},
};

fn exit(status: ExitStatus) -> Result<i32> {
    status
        .code()
        .or_else(|| status.signal().map(|s| -s))
        .ok_or_else(|| "missing process status".into())
}
pub fn mdk(binary: &Path) -> Result<Value> {
    let names = stdout(Command::new(binary).args(["--list", "--format", "terse"]))?;
    let mut cases = serde_json::Map::new();
    for name in names.lines().filter_map(|line| line.strip_suffix(": test")) {
        let result = capture(Command::new(binary).args(["--exact", name]), 60, None)?;
        require(
            cases
                .insert(name.to_owned(), json!(exit(result.status)?))
                .is_none(),
            "duplicate MDK case",
        )?;
    }
    require(!cases.is_empty(), "no MDK test cases discovered")?;
    let temp = tempfile::tempdir()?;
    let env = Path::new("/usr/bin/env");
    let copy = temp.path().join("env");
    fs::copy(env, &copy)?;
    let mut controls = serde_json::Map::new();
    for (key, path) in [("system", env), ("copy", copy.as_path())] {
        let result = capture(Command::new(path).env_clear(), 10, None)?;
        controls.insert(key.into(),json!({"exit":exit(result.status)?,"stdoutBytes":result.stdout.len(),"stderrBytes":result.stderr.len()}));
    }
    Ok(json!({"environmentExecutable":controls,"freshProcessPerTest":cases}))
}
pub fn counted(name: &str, runs: usize, raw: &str, passing: &[&str]) -> bool {
    let summaries = raw.lines().filter(|line| line.starts_with("test result:"));
    runs > 0
        && passing.len() == runs
        && passing.iter().all(|n| *n == name)
        && summaries.clone().count() == runs
        && summaries
            .into_iter()
            .all(|line| line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;"))
}

fn parser_result(name: &str, raw: &str) -> Result<()> {
    let names = raw
        .lines()
        .filter_map(|line| line.strip_prefix("test ")?.strip_suffix(" ... ok"))
        .collect::<Vec<_>>();
    require(
        counted(name, 1, raw, &names),
        format!("expected one actual execution of {name}"),
    )
}

pub fn parser_cases(product: &Path) -> Result<Value> {
    let (_, prefix) =
        public_data_qualification::manifest_product::leaf(&product.join("marmish-native"))?;
    let mut counts = std::collections::BTreeMap::new();
    for case in public_data_qualification::manifest_product::CASES {
        let registered = format!("{prefix}::tests::{case}");
        let raw = stdout(Command::new("cargo").current_dir(product).args([
            "test",
            "--locked",
            "--offline",
            "--release",
            "-p",
            "marmish-native",
            "--lib",
            "--",
            "--exact",
            &registered,
        ]))?;
        parser_result(&registered, &raw)?;
        counts.insert(format!("native_manifest::tests::{case}"), 1);
    }
    Ok(json!({"harness":"marmish-native/lib/marmish_native","cases":counts}))
}

#[cfg(test)]
mod parser_controls {
    use super::*;
    #[test]
    fn requires_one_passing_execution_of_the_selected_case() {
        let name = "native_manifest::__projection_10::tests::strict_vectors";
        let success =
            format!("test {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;");
        assert!(parser_result(name, &success).is_ok());
        for raw in [
            "test result: ok. 0 passed; 0 failed; 0 ignored;".to_owned(),
            success.replace(name, "other"),
            format!("{success}\ntest result: ok. 0 passed; 0 failed; 0 ignored;"),
            format!("{success}\n{success}"),
        ] {
            assert!(parser_result(name, &raw).is_err());
        }
    }
}

pub fn child(original: &Path, compiler: &Path) -> Result<Value> {
    const LOW: &str = "process_limits_tests::lower_soft_hard_and_descriptor_flags_are_unchanged";
    let manifest = portable().join("engine-process-limits-package.json");
    let work = tempfile::tempdir()?;
    let pkg = work.path().join("marmish-engine");
    run(Command::new(compiler)
        .args(["build", "native-package", "rust"])
        .arg(&manifest)
        .arg(&pkg))?;
    fs::create_dir(work.path().join("marmish-vault"))?;
    super::suite_stage::vault_engine(original, work.path())?;
    let test = || {
        stdout(Command::new("cargo").current_dir(&pkg).args([
            "+1.97.1",
            "test",
            "--offline",
            "--locked",
            "--release",
            "--test",
            "selected_upstream",
            "--",
            "--exact",
            LOW,
            "--format",
            "pretty",
        ]))
    };
    let passed = Regex::new(r"(?m)^test ([\w:]+) \.\.\. ok$")?;
    let baseline = test()?;
    let names = passed
        .captures_iter(&baseline)
        .map(|c| c.get(1).unwrap().as_str())
        .collect::<Vec<_>>();
    require(
        counted(LOW, 2, &baseline, &names),
        "parent and child did not both pass",
    )?;
    let path = pkg.join("tests/selected_upstream.rs");
    let src = fs::read_to_string(&path)?;
    let filter = r#"&format!("process_limits_tests::{name}")"#;
    require(src.matches(filter).count() == 1, "unexpected child filter")?;
    fs::write(
        path,
        src.replace(filter, r#"&format!("missing_child::{name}")"#),
    )?;
    let mutant = test()?;
    let names = passed
        .captures_iter(&mutant)
        .map(|c| c.get(1).unwrap().as_str())
        .collect::<Vec<_>>();
    require(
        names == [LOW]
            && mutant.contains("test result: ok. 0 passed;")
            && !counted(LOW, 2, &mutant, &names),
        "zero-test child control did not discriminate",
    )?;
    Ok(
        json!({"case":LOW,"packageManifestSha256":sha(&manifest)?,"baselinePassingProcesses":2,
        "mutantPassingProcesses":1,"childMatchedZero":true,"mutantCargoExit":0,"coverageGateRejected":true}),
    )
}
