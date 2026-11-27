use super::*;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;
use tempfile::tempdir;

fn cases(input: &str) -> Result<Vec<Value>> {
    Ok(serde_json::from_str(input)?)
}

fn observe(binary: &Path, input: &[u8]) -> Result<String> {
    // A file avoids deadlock when an observer writes output before consuming all input.
    let scratch = tempdir()?;
    let path = scratch.path().join("input");
    fs::write(&path, input)?;
    output(Command::new(binary).stdin(Stdio::from(fs::File::open(path)?)))
}

pub fn policy(native: &Path, generated: &Path) -> Result<Value> {
    let cases = cases(include_str!("../policy-cases.json"))?;
    let mut input = Vec::new();
    for case in &cases {
        writeln!(input, "{}", case["input"].as_str().ok_or("missing input")?)?;
    }
    let original = observe(native, &input)?;
    let candidate = observe(generated, &input)?;
    for (label, observations) in [("native", &original), ("generated", &candidate)] {
        if observations.lines().count() != cases.len() {
            return Err(format!("{label} observation count differs").into());
        }
        for (case, actual) in cases.iter().zip(observations.lines()) {
            let expected = case["expected"].as_str().ok_or("missing expectation")?;
            if expected != actual {
                return Err(format!(
                    "{label} {}: expected {expected:?}, observed {actual:?}",
                    case["name"]
                )
                .into());
            }
        }
    }
    Ok(
        json!({"cases": cases.len(), "inputsSha256": sha(&input), "observationsSha256": sha(original.as_bytes())}),
    )
}

pub fn room(observer: &Path) -> Result<usize> {
    let cases = cases(include_str!("../room-cases.json"))?;
    let scratch = tempdir()?;
    let path = scratch.path().join("config.json");
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    for case in &cases {
        let mut config = json!({"version":1,"role":"server","relays":["wss://relay.example/"],"tor":"auto","trusted_remotes":[],"execution_clients":[],"cwd":"/","limits":{},"aliases":{}});
        for (name, value) in case["fields"].as_object().ok_or("invalid room fields")? {
            config[name] = value.clone();
        }
        write_json(&path, &config)?;
        let actual = output(Command::new(observer).arg(scratch.path()))?;
        let expected = case["expected"]
            .as_str()
            .ok_or("missing room expectation")?;
        if actual.trim_end_matches('\n') != expected {
            return Err(format!(
                "{}: expected {expected:?}, observed {actual:?}",
                case["name"]
            )
            .into());
        }
    }
    Ok(cases.len())
}
