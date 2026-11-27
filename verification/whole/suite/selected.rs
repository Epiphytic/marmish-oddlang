//! Exercise staged upstream library cases in the producer's isolated workspace.
use super::suite_controls::counted;
use public_data_qualification::{Result, require, stdout};
use serde_json::{Map, Value, json};
use std::{path::Path, process::Command};

const VAULT: &str = "tests::strict_public_values";
const LOW: &str = "process_limits_tests::lower_soft_hard_and_descriptor_flags_are_unchanged";
const LINUX: &str = "process_limits_tests::linux_process_limits_are_unchanged";
const READER: &str = "secret_request_tests::private_reader_wipes_consumed_spans";
const MDK: &str = "tests::synthetic_json_and_ndjson_are_bounded_and_structural_only";

struct Harness {
    package: &'static str,
    prefix: &'static str,
    runs: usize,
    cases: Vec<&'static str>,
}

impl Harness {
    fn cargo(&self, product: &Path, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(product).args([
            "test",
            "--locked",
            "--offline",
            "--release",
            "-p",
            self.package,
        ]);
        if self.prefix.is_empty() {
            cmd.args(["--test", "selected_upstream"]);
        } else {
            cmd.arg("--lib");
        }
        stdout(cmd.arg("--").args(args))
    }
}

fn passing(raw: &str) -> Vec<&str> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("test ")?.strip_suffix(" ... ok"))
        .collect()
}

fn zero_control(selector: &str, raw: &str) -> Result<Value> {
    let names = passing(raw);
    let summaries: Vec<_> = raw
        .lines()
        .filter(|line| line.starts_with("test result:"))
        .collect();
    require(
        names.is_empty()
            && matches!(summaries.as_slice(), [line] if line.starts_with("test result: ok. 0 passed; 0 failed; 0 ignored;"))
            && !counted(selector, 1, raw, &names),
        "absent selector did not produce exactly one zero-test result",
    )?;
    Ok(json!({"selector":selector,"cargoExit":0,"executed":0,"coverageRejected":true}))
}

pub fn exercise(product: &Path, host: &str, suite: &str) -> Result<Value> {
    let engine = match host {
        "aarch64-apple-darwin" => vec![LOW],
        "aarch64-unknown-linux-gnu" => vec![LINUX, LOW],
        _ => return Err(format!("unsupported selection host {host}").into()),
    };
    require(
        matches!(suite, "recovered" | "seven" | "manifest" | "manifest-mdk"),
        "unknown suite selection",
    )?;
    let mut harnesses = vec![
        Harness {
            package: "marmish-vault",
            prefix: "",
            runs: 1,
            cases: vec![VAULT],
        },
        Harness {
            package: "marmish-engine",
            prefix: "",
            runs: 2,
            cases: engine,
        },
    ];
    if suite != "recovered" {
        harnesses.push(Harness {
            package: "marmish-prototype",
            prefix: "buffer::__projection_0::",
            runs: 1,
            cases: vec![READER],
        });
    }
    if suite == "manifest-mdk" {
        harnesses.push(Harness {
            package: "marmish-mdk",
            prefix: "metadata::__projection_2::",
            runs: 1,
            cases: vec![MDK],
        });
    }
    let mut processes = Map::new();
    let mut witnessed = Map::new();
    let mut control = Value::Null;
    for harness in harnesses {
        let listing = harness.cargo(product, &["--list"])?;
        let mut registered: Vec<_> = listing
            .lines()
            .filter_map(|line| line.strip_suffix(": test"))
            .collect();
        registered.sort_unstable();
        let expected: Vec<_> = harness
            .cases
            .iter()
            .map(|case| format!("{}{case}", harness.prefix))
            .collect();
        require(
            registered == expected,
            format!(
                "unexpected upstream tests registered for {}",
                harness.package
            ),
        )?;
        let mut counts = Map::new();
        for (case, name) in harness.cases.iter().zip(expected) {
            let raw = harness.cargo(product, &["--exact", &name, "--format", "pretty"])?;
            require(
                counted(&name, harness.runs, &raw, &passing(&raw)),
                format!("expected {} executions of {name}", harness.runs),
            )?;
            counts.insert((*case).into(), json!(harness.runs));
            if *case == MDK {
                let selector = format!("{name}_absent");
                control = zero_control(
                    &selector,
                    &harness.cargo(product, &["--exact", &selector, "--format", "pretty"])?,
                )?;
            }
        }
        let key = format!(
            "{}/lib/{}",
            harness.package,
            harness.package.replace('-', "_")
        );
        witnessed.insert(key.clone(), json!(harness.cases));
        processes.insert(key, Value::Object(counts));
    }
    Ok(json!({"passingProcesses":processes,"witnessed":witnessed,"mdkZeroTestControl":control}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_selector_requires_exactly_one_successful_empty_harness() {
        let empty = "test result: ok. 0 passed; 0 failed; 0 ignored; 1 filtered out";
        assert!(zero_control("absent", empty).is_ok());
        for bad in [
            String::new(),
            format!("{empty}\n{empty}"),
            format!("test other ... ok\n{empty}"),
            format!("{empty}\ntest result: FAILED."),
        ] {
            assert!(zero_control("absent", &bad).is_err());
        }
    }
}
