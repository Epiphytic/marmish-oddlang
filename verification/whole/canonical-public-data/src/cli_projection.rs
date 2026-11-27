//! Common recovery and build checks for parser-based native clients.
use crate::{Result, assembly::bytes_sha, require, run};
use std::{collections::BTreeSet, fs, path::Path, process::Command};
pub const REVISION: &str = "af54339d250290490d62d4475a77539b7e739071";
pub fn pinned(original: &Path, path: &str, digest: &str) -> Result<Vec<u8>> {
    let bytes = run(Command::new("git")
        .current_dir(original)
        .args(["show", &format!("{REVISION}:{path}")]))?;
    require(
        bytes_sha(&bytes) == digest,
        format!("source pin differs for {path}"),
    )?;
    Ok(bytes)
}

pub fn registry(lock: &[u8]) -> Result<BTreeSet<[String; 4]>> {
    package_pins(lock, |p| {
        p.get("source")
            .and_then(|s| s.as_str())
            .is_some_and(|s| s.starts_with("registry+"))
    })
}

pub fn checksummed_packages(lock: &[u8]) -> Result<BTreeSet<[String; 4]>> {
    package_pins(lock, |p| p.get("checksum").is_some())
}

fn package_pins(
    lock: &[u8],
    include: impl Fn(&toml::Value) -> bool,
) -> Result<BTreeSet<[String; 4]>> {
    let doc: toml::Table = std::str::from_utf8(lock)?.parse()?;
    let mut pins = BTreeSet::new();
    for package in doc
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("expected lock packages")?
    {
        if include(package) {
            let field = |name: &str| -> Result<String> {
                Ok(package
                    .get(name)
                    .and_then(|s| s.as_str())
                    .ok_or("missing registry pin")?
                    .into())
            };
            pins.insert([
                field("name")?,
                field("version")?,
                field("source")?,
                field("checksum")?,
            ]);
        }
    }
    Ok(pins)
}

pub fn recover(compiler: &Path, original: &Path, inputs: &Path) -> Result<()> {
    let git = |args: &[&str]| run(Command::new("git").current_dir(original).args(args));
    require(
        std::str::from_utf8(&git(&["rev-parse", "HEAD"])?)?.trim_end() == REVISION,
        "unexpected original revision",
    )?;
    let manifest = pinned(
        original,
        "crates/marmish-native/Cargo.toml",
        "f92cdde82b3e4cf077294a7296117af4dcc28dba55bf1c9d694d7a4b273f539f",
    )?;
    let manifest: toml::Table = std::str::from_utf8(&manifest)?.parse()?;
    for (field, value) in [
        ("name", "marmish-native"),
        ("version", "0.3.1"),
        ("edition", "2024"),
    ] {
        require(
            manifest["package"][field].as_str() == Some(value),
            "unexpected original package metadata",
        )?;
    }
    let available = registry(&pinned(
        original,
        "Cargo.lock",
        "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    )?)?;
    let required = registry(&fs::read(inputs.join("parser-Cargo.lock"))?)?;
    require(
        !required.is_empty() && required.is_subset(&available),
        "parser dependency pins differ from the original release",
    )?;
    let tmp = tempfile::tempdir()?;
    let source = tmp.path().join("parser.rs");
    fs::write(
        &source,
        pinned(
            original,
            "crates/marmish-native/src/cli.rs",
            "a4122418f4f777a53e839487b3b162951ac2b1447ea26b968245322390c866d4",
        )?,
    )?;
    let graph = tmp.path().join("candidate.json");
    run(Command::new(compiler)
        .args(["recover", "cli-surface"])
        .arg(source)
        .arg(inputs.join("selection.json"))
        .arg(&graph))?;
    require(
        fs::read(graph)? == fs::read(inputs.join("graph.json"))?,
        "recovered graph differs from published candidate",
    )
}

pub fn generate(
    compiler: &Path,
    manifest: &Path,
    output: &Path,
    expected: &Path,
    locks: &[(&str, &str)],
) -> Result<()> {
    run(Command::new(compiler.canonicalize()?)
        .args(["build", "native-workspace", "rust"])
        .arg(manifest)
        .arg(output))?;
    for (file, reference) in locks {
        require(
            fs::read(output.join(file))? == fs::read(expected.join(reference))?,
            format!("generated {file} differs from the reviewed lock"),
        )?;
    }
    run(Command::new("cargo").current_dir(output).args([
        "build",
        "--release",
        "--locked",
        "--offline",
        "--bins",
    ]))?;
    Ok(())
}

pub fn input_bytes(
    base: &Path,
    descriptor: &serde_json::Value,
) -> Result<(std::path::PathBuf, Vec<u8>)> {
    let path = base
        .join(crate::text(&descriptor["path"])?)
        .canonicalize()?;
    require(path.starts_with(base), "input pin escapes its base")?;
    let bytes = fs::read(&path)?;
    require(
        descriptor["sha256"] == bytes_sha(&bytes),
        "invalid input pin",
    )?;
    Ok((path, bytes))
}
pub fn recover_status(compiler: &Path, original: &Path, portable: &Path) -> Result<()> {
    recover(compiler, original, &portable.join("native-cli-v1"))?;
    for (name, digest) in [
        (
            "crates/marmish-native/src/ipc.rs",
            "ffeeba6b2d0b8fef1d44bea311e5548e8b7ac520d4b7d04286077547273b8a4a",
        ),
        (
            "crates/marmish-native/src/error.rs",
            "54eb8a335f91cb09de6ecd8d598acdec3476ee7067968333f85c824a5fee2254",
        ),
        (
            "crates/marmish-native/src/transport.rs",
            "5490e5d39761df1fc386249c3e901a920c4343257836908a6f46746a22c06800",
        ),
    ] {
        pinned(original, name, digest)?;
    }
    let lock = pinned(
        original,
        "Cargo.lock",
        "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3",
    )?;
    require(
        registry(&fs::read(
            portable.join("native-status-v1/client-Cargo.lock"),
        )?)?
        .is_subset(&registry(&lock)?),
        "status dependencies differ from the reference",
    )
}
