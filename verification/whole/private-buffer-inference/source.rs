//! Recover reader traces from independently mutated copies of the pinned upstream source.
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, bytes_sha},
    buffered::{SOURCE, reader_checkout},
    require, stdout, whole,
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
const CASE: &str = "portable_buffer_behavior::collect";
#[derive(Clone, Copy)]
enum Change {
    Empty,
    Consume,
    Error,
}
use Change::*;
const LANES: [(&str, &[Change]); 5] = [
    ("original", &[]),
    ("empty", &[Empty]),
    ("consume", &[Consume]),
    ("error", &[Error]),
    ("combined", &[Empty, Consume, Error]),
];
fn changed(source: &str, changes: &[Change]) -> Result<String> {
    let mut result = source.to_owned();
    for change in changes {
        let (old, new) = match change {
            Empty => (
                "let available = self.fill_buf()?;",
                "if output.is_empty() { return Ok(0); } let available = self.fill_buf()?;",
            ),
            Consume => ("self.bytes[self.start..end].zeroize();", ""),
            Error => (
                "self.end = self.inner.read(self.bytes.as_mut())?;",
                "self.end = 0; self.end = self.inner.read(self.bytes.as_mut())?;",
            ),
        };
        require(
            result.matches(old).count() == 1,
            format!("ambiguous source change at {old}"),
        )?;
        result = result.replace(old, new);
    }
    Ok(result)
}
pub fn prepare(original: &Path, checkout: &Path) -> Result<Value> {
    reader_checkout(original, checkout)?;
    let file = checkout.join(SOURCE);
    let source = changed(&fs::read_to_string(&file)?, &[Empty, Consume, Error])?;
    fs::write(file, &source)?;
    Ok(json!(bytes_sha(source.as_bytes())))
}
fn parse(raw: &str, name: &str) -> Result<Value> {
    let passed = format!("test {name} ... ok");
    require(
        raw.lines().filter(|l| *l == passed).count() == 1,
        "harness did not execute the exact observation once",
    )?;
    let mut trace = json!({"format":"oddlang-buffered-input-trace-v1"});
    for key in ["emptyRead", "consume", "refillError"] {
        let prefix = format!("{key} ");
        let mut lines = raw.lines().filter_map(|l| l.strip_prefix(&prefix));
        let row = lines
            .next()
            .ok_or_else(|| format!("missing trace row {key}"))?;
        require(lines.next().is_none(), format!("duplicate trace row {key}"))?;
        trace[key] = serde_json::from_str(row)?;
    }
    Ok(trace)
}
pub fn capture(checkout: &Path, target: &Path, host: &str, name: &str) -> Result<Value> {
    let raw = stdout(
        Command::new("cargo")
            .current_dir(checkout)
            .args([
                "+1.97.1",
                "test",
                "--release",
                "--locked",
                "--offline",
                "--target-dir",
            ])
            .arg(target)
            .args([
                "--target",
                host,
                "-p",
                "marmish-prototype",
                "--lib",
                "--",
                "--exact",
                name,
                "--nocapture",
            ]),
    )?;
    parse(&raw, name)
}
pub fn observe(original: &Path, target: &Path) -> Result<Value> {
    let toolchain = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = toolchain
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("missing native host")?;
    let observer = fs::read_to_string(whole().join("private-buffer-inference/trace.rs"))?;
    let temp = tempfile::tempdir()?;
    let checkout = temp.path().join("reference");
    reader_checkout(original, &checkout)?;
    let file = checkout.join(SOURCE);
    let source = fs::read_to_string(&file)?;
    let mut lanes = serde_json::Map::new();
    for (lane, changes) in LANES {
        let version = changed(&source, changes)?;
        fs::write(&file, format!("{version}\n{observer}"))?;
        lanes.insert(lane.into(),json!({"sourceSha256":bytes_sha(version.as_bytes()),"trace":capture(&checkout,target,host,CASE)?}));
    }
    Ok(
        json!({"revision":ORIGINAL,"target":host,"observerSha256":bytes_sha(observer.as_bytes()),"lanes":lanes}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_one_passing_case_and_one_of_each_row() {
        let raw = "test t ... ok\nemptyRead {}\nconsume {}\nrefillError {}\n";
        assert!(parse(raw, "t").is_ok());
        for extra in ["test t ... ok\n", "consume {}\n"] {
            assert!(parse(&format!("{raw}{extra}"), "t").is_err());
        }
        assert!(parse(raw, "absent").is_err());
        assert!(parse(&raw.replace("consume {}\n", ""), "t").is_err());
    }
}
