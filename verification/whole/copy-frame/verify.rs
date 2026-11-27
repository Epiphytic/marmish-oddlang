//! Qualify frame APIs, wire bytes and error order on the original and sealed generated library.
use clap::{Parser, Subcommand};
use copy_checkpoint_replay::{build_compiler, compare, consumer, corpus, stage};
use public_data_qualification::{
    assembly::{ORIGINAL, Sandbox, bytes_sha, checkout, root},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "20f26a01bda33a1c6a6b284839dbf417f88af850";
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";
const HARNESS: &str = r#"#[cfg(test)]
mod copy_protocol {
    pub use marmish_stream::{MAX_BLOCK, MAX_OFFSET, codec::Codec, copy_protocol::*};
    pub use sha2::{Digest,Sha256};
    const CONTEXT_DOMAIN: &[u8] = b"marmish-copy-context/1\0";
    include!("upstream.rs");
}
#[cfg(test)]
mod exact;
"#;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Qualify {
        original: PathBuf,
        framework: PathBuf,
        receipt: PathBuf,
    },
    /// Replay the same original tests against a member of a larger generated workspace.
    TestPackage {
        original: PathBuf,
        package: PathBuf,
        lock: PathBuf,
        work: PathBuf,
    },
}
fn upstream_tests(original: &Path) -> Result<String> {
    let src = stdout(
        Command::new("git")
            .current_dir(original)
            .args(["show", &format!("{ORIGINAL}:{SOURCE}")]),
    )?;
    let marker = "#[cfg(test)]\nmod tests {";
    require(
        src.matches(marker).count() == 1,
        "ambiguous original test module",
    )?;
    let start = src.find(marker).ok_or("missing upstream tests")?;
    let (module, _) = src[start..]
        .split_once("\nmod path_encoding {")
        .ok_or("missing end of upstream tests")?;
    require(
        module.matches("#[test]").count() == 11,
        "expected eleven upstream tests",
    )?;
    Ok(module.to_owned())
}

fn tests(
    package: &Path,
    lock: &Path,
    dir: &Path,
    module: &str,
    negative: bool,
) -> Result<Vec<String>> {
    let src = dir.join("src");
    fs::create_dir_all(&src)?;
    fs::write(src.join("upstream.rs"), module)?;
    fs::write(src.join("exact.rs"), include_bytes!("exact.rs"))?;
    fs::write(src.join("lib.rs"), HARNESS)?;
    let dep = serde_json::to_string(&package.canonicalize()?)?;
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname=\"verify-copy-frame\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nsha2=\"=0.10.9\"\nmarmish-stream={{path={dep}}}\n[workspace]\n"
        ),
    )?;
    fs::copy(lock, dir.join("Cargo.lock"))?;
    let cargo = || {
        let mut c = Command::new("cargo");
        c.current_dir(dir).arg("+1.97.1");
        c
    };
    run(cargo().args(["metadata", "--offline", "--format-version", "1"]))?;
    let test = || {
        let mut c = cargo();
        c.args([
            "test",
            "--locked",
            "--offline",
            "--release",
            "--lib",
            "--target-dir",
        ])
        .arg("target");
        c
    };
    if negative {
        for (case, actual, wanted) in [
            (
                "exact::data_and_progress_guards_have_directional_precedence",
                "Offset",
                "Limit",
            ),
            (
                "exact::data_decode_rejects_offset_before_empty_body",
                "Limit",
                "Offset",
            ),
        ] {
            let output = capture(test().arg(case).args(["--", "--exact"]), 600, None)?;
            let log = String::from_utf8(output.stdout)?;
            require(
                output.status.code() == Some(101)
                    && log.lines().any(|l| l == format!("test {case} ... FAILED"))
                    && log
                        .lines()
                        .any(|l| l.trim() == format!("left: Some({actual})"))
                    && log
                        .lines()
                        .any(|l| l.trim() == format!("right: Some({wanted})")),
                format!("guard mutation did not fail {case} as expected"),
            )?;
        }
        return Ok(Vec::new());
    }
    let log = stdout(&mut test())?;
    let mut names: Vec<String> = log
        .lines()
        .filter_map(|l| Some(l.strip_prefix("test ")?.strip_suffix(" ... ok")?.to_owned()))
        .collect();
    names.sort();
    require(
        names.len() == 20
            && names
                .iter()
                .filter(|n| n.starts_with("copy_protocol::tests::"))
                .count()
                == 11
            && names.iter().filter(|n| n.starts_with("exact::")).count() == 9,
        "unexpected copy test suite",
    )?;
    Ok(names)
}
fn repeat_records(package: &Path, work: &Path) -> Result<Value> {
    let manifest = copy_manifest_replay::cases();
    let (records, wires) = corpus::cases();
    let (requests, expected, _) = corpus::expectations(&records, &wires);
    let mut counts = json!({});
    for (name, req, want, constructions, decodes) in [
        (
            "manifest",
            &manifest.requests,
            &manifest.expected,
            manifest.metadata["constructions"]
                .as_u64()
                .ok_or("missing construction count")?,
            manifest.metadata["decodes"]
                .as_u64()
                .ok_or("missing decode count")?,
        ),
        (
            "checkpoint",
            &requests,
            &expected,
            records.len() as u64,
            wires.len() as u64,
        ),
    ] {
        let caller = whole().join(format!("copy-{name}/caller.rs"));
        let exe = consumer(
            package,
            &work.join(name),
            &package.join("Cargo.lock"),
            &caller,
        )?;
        compare(name, &exe, req, want)?;
        counts[name] = json!({"constructions":constructions,"decodes":decodes});
    }
    Ok(counts)
}
fn qualify(original: &Path, framework: &Path, receipt: &Path) -> Result {
    let original = original.canonicalize()?;
    let framework = framework.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, FRAMEWORK, false)?;
    let private = tempfile::tempdir()?;
    let scratch = private.path().canonicalize()?;
    let compiler = build_compiler(&framework, &scratch.join("tools"), "+1.97.1")?;
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let cli = work.join("oddlang");
    fs::copy(compiler, &cli)?;
    let (plan, mut manifest) = stage(
        &work.join("inputs"),
        "stream-copy-frame-package.json",
        false,
    )?;
    let cache = work.join("provider-cache");
    fs::create_dir(&cache)?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&root().canonicalize()?, &original, &framework, &scratch],
        &cache,
    )?;
    let project = |label: &str| -> Result<PathBuf> {
        let out = work.join(label);
        sandbox.deny(&[
            original.join("Cargo.toml"),
            whole().join("copy-frame/exact.rs"),
        ])?;
        run(sandbox
            .command(&work)?
            .arg(&cli)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&out))?;
        run(sandbox
            .command(&out)?
            .args(["cargo", "build", "--release", "--locked", "--offline"]))?;
        Ok(out)
    };
    let native = project("native")?;
    let lock = native.join("package.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join("copy-frame-v1/package.lock"))?,
        "frame lock differs",
    )?;
    let module = upstream_tests(&original)?;
    let reference = tests(
        &original.join("crates/marmish-stream"),
        &original.join("Cargo.lock"),
        &scratch.join("original"),
        &module,
        false,
    )?;
    let generated = tests(
        &native,
        &native.join("Cargo.lock"),
        &scratch.join("generated"),
        &module,
        false,
    )?;
    require(
        generated == reference,
        "original and generated test suites differ",
    )?;
    let repeated = repeat_records(&native, &scratch.join("nested"))?;
    let components = manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    let mut frames = components
        .iter_mut()
        .filter(|c| c["kind"] == "binary-frame");
    let pin = &mut frames.next().ok_or("missing frame graph")?["graph"];
    require(frames.next().is_none(), "ambiguous frame graph")?;
    let path = plan
        .parent()
        .ok_or("missing input directory")?
        .join(text(&pin["path"])?);
    let mut graph = load(&path)?;
    let mut rows = graph["cases"]
        .as_object_mut()
        .ok_or("missing cases")?
        .values_mut()
        .filter(|c| c["tag"] == 5);
    let data = rows.next().ok_or("missing Data tag")?;
    require(rows.next().is_none(), "duplicated Data tag")?;
    data["encodeGuards"]
        .as_array_mut()
        .ok_or("missing encode guards")?
        .reverse();
    data["slots"][0]["encoding"]["maximum"] = json!(u64::MAX);
    fs::write(&path, json_bytes(&graph)?)?;
    pin["sha256"] = json!(sha(&path)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let mutant = project("mutant")?;
    tests(
        &mutant,
        &mutant.join("Cargo.lock"),
        &scratch.join("negative"),
        &module,
        true,
    )?;
    let report = json!({
        "original":ORIGINAL,"framework":FRAMEWORK,"rust":rust.trim_end(),
        "originalAndCompanionReadDenied":true,"packageLockSha256":sha(&lock)?,
        "upstreamTestModuleSha256":bytes_sha(module.as_bytes()),"exactTestsSha256":bytes_sha(include_bytes!("exact.rs")),
        "upstreamTests":11,"exactTests":9,"testNames":generated,"repeatedStandaloneEvidence":repeated,
        "negativeControl":"reordering Data encode guards and deferring the early offset bound reverse exact Offset and Limit precedence"
    });
    fs::write(receipt, json_bytes(&report)?)?;
    Ok(())
}
fn main() -> Result {
    match Args::parse().action {
        Action::Qualify {
            original,
            framework,
            receipt,
        } => qualify(&original, &framework, &receipt),
        Action::TestPackage {
            original,
            package,
            lock,
            work,
        } => {
            println!(
                "{}",
                serde_json::to_string(&tests(
                    &package,
                    &lock,
                    &work,
                    &upstream_tests(&original)?,
                    false
                )?)?
            );
            Ok(())
        }
    }
}
