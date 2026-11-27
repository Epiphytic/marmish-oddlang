//! Observe local reader semantics in a disposable clone of the pinned upstream.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, bytes_sha},
    buffered::{CASES, SOURCE, reader_source},
    capture, require, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    target: PathBuf,
    receipt: PathBuf,
}

fn case(work: &Path, target: &Path, host: &str, name: &str, failure: Option<&str>) -> Result<i32> {
    let output = capture(
        Command::new("cargo")
            .current_dir(work)
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
            ]),
        600,
        None,
    )?;
    let (status, label) = if failure.is_some() {
        (101, "FAILED")
    } else {
        (0, "ok")
    };
    let log = String::from_utf8(output.stdout)?;
    require(
        output.status.code() == Some(status)
            && log
                .lines()
                .any(|line| line == format!("test {name} ... {label}"))
            && failure.is_none_or(|assertion| log.contains(assertion)),
        format!(
            "unexpected outcome for {name}\n{log}\n{}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(status)
}
fn observe(original: &Path, target: &Path) -> Result<Value> {
    let suite = fs::read_to_string(whole().join("private-buffer/observations.rs"))?;
    let source = reader_source(original)?;
    let rustc = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let mut hosts = rustc.lines().filter_map(|l| l.strip_prefix("host: "));
    let host = hosts.next().ok_or("missing native host")?;
    require(hosts.next().is_none(), "ambiguous native host")?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().join("reference");
    public_data_qualification::buffered::reader_checkout(original, &work)?;
    let stage = |text: &str| fs::write(work.join(SOURCE), format!("{text}\n{suite}"));
    stage(&source)?;
    for name in CASES {
        case(&work, target, host, name, None)?;
    }
    let mut controls = serde_json::Map::new();
    for (label, old, new, name, assertion) in [
        (
            "retainConsumedBytes",
            "self.bytes[self.start..end].zeroize();",
            "",
            CASES[1],
            "consumed span must be erased",
        ),
        (
            "clearEndBeforeRefill",
            "self.end = self.inner.read(self.bytes.as_mut())?;",
            "self.end = 0; self.end = self.inner.read(self.bytes.as_mut())?;",
            CASES[2],
            "failed refill preserves old end",
        ),
    ] {
        require(
            source.matches(old).count() == 1,
            format!("ambiguous source mutation {label}"),
        )?;
        stage(&source.replace(old, new))?;
        let exit = case(&work, target, host, name, Some(assertion))?;
        controls.insert(
            label.to_string(),
            json!({"test":name,"expectedAssertion":assertion,"exit":exit}),
        );
    }
    Ok(
        json!({"revision":ORIGINAL,"rustc":rustc.trim(),"target":host,"upstreamFileSha256":bytes_sha(source.as_bytes()),
        "callerSha256":bytes_sha(suite.as_bytes()),"passed":CASES,"controls":controls}),
    )
}
fn main() -> Result<()> {
    let args = Args::parse();
    let report = observe(
        &args.original.canonicalize()?,
        &std::path::absolute(args.target)?,
    )?;
    fs::write(
        args.receipt,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}
