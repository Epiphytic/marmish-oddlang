//! Stage the core package graph and verify its generated Rust source inventory.
#[path = "../../../core-package/linux.rs"]
mod linux;
#[path = "../../../core-package/macos.rs"]
mod macos;
#[path = "../../../core-package/mutations.rs"]
mod mutations;
use clap::{Parser, Subcommand};
use mutations::Mutation;
use public_data_qualification::{
    Result, cli_projection::input_bytes, list, load, product::file_digests, require, run, sha,
    stdout, text,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const FRAMEWORK: &str = "a635eec8e0f71d2dabc9aee28655e26494a7506f";
const RUST: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
fn checkout(root: &Path, pin: &str) -> Result {
    require(
        stdout(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == pin,
        "wrong checkout revision",
    )?;
    require(
        run(Command::new("git").current_dir(root).args([
            "status",
            "--porcelain",
            "--untracked-files=all",
        ]))?
        .is_empty(),
        "checkout is dirty",
    )
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Run the source-isolated Linux qualification.
    Linux(linux::Args),
    /// Run the source-isolated macOS qualification and mutation witnesses.
    Macos(macos::Args),
    /// Prepare the private Cargo home and emit the reviewed read roots and build environment.
    MacosEnvironment {
        work: PathBuf,
        host: String,
    },
    /// Emit the same Seatbelt policy for a caller that adds its own read roots and exclusions.
    MacosPolicy {
        work: PathBuf,
        #[arg(long)]
        read: Vec<PathBuf>,
        #[arg(long)]
        deny: Vec<PathBuf>,
    },
    #[command(hide = true)]
    ProbeLoopback {
        port: u16,
    },
    Compare {
        original: PathBuf,
        generated: PathBuf,
    },
    Observe {
        transcript: PathBuf,
        family: String,
        name: String,
    },
    Stage {
        portable: PathBuf,
        destination: PathBuf,
        #[arg(long, value_enum)]
        mutation: Option<Mutation>,
    },
    Verify {
        package: PathBuf,
        manifest: PathBuf,
    },
}

fn stage(portable: &Path, destination: &Path, mutation: Option<Mutation>) -> Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let scratch = tempfile::tempdir_in(parent)?;
    let staged = scratch.path().join("inputs");
    let root = portable.canonicalize()?;
    fs::create_dir(&staged)?;
    let manifest = staged.join("core-package.json");
    let source_manifest = root.join("core-package.json").canonicalize()?;
    require(
        source_manifest.starts_with(&root),
        "manifest escapes portable directory",
    )?;
    let manifest_bytes = fs::read(source_manifest)?;
    let spec: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let copy = |pin: &serde_json::Value| -> Result<()> {
        let relative = Path::new(text(&pin["path"])?);
        require(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "unsafe input path",
        )?;
        let (_, bytes) = input_bytes(&root, pin)?;
        let out = staged.join(relative);
        fs::create_dir_all(out.parent().ok_or("missing parent")?)?;
        fs::write(out, bytes)?;
        Ok(())
    };
    copy(&spec["cargoLock"])?;
    for component in list(&spec["components"])? {
        for field in ["graph", "overlay"] {
            copy(&component[field])?;
        }
    }
    use std::io::Write;
    fs::File::create_new(&manifest)?.write_all(&manifest_bytes)?;
    if let Some(mutation) = mutation {
        mutations::apply(&staged, mutation)?;
    }
    renameat_with(CWD, &staged, CWD, destination, RenameFlags::NOREPLACE)?;
    Ok(())
}

fn verify(package: &Path, manifest: &Path) -> Result<()> {
    let expected =
        BTreeSet::from(["Cargo.lock", "Cargo.toml", "package.lock", "src"].map(String::from));
    let mut actual = BTreeSet::new();
    for e in fs::read_dir(package)? {
        let e = e?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| "invalid package filename")?;
        let ty = e.file_type()?;
        require(
            if name == "src" {
                ty.is_dir()
            } else {
                ty.is_file()
            },
            "unexpected generated package entry type",
        )?;
        actual.insert(name);
    }
    require(actual == expected, "unexpected generated package entries")?;
    let spec = load(manifest)?;
    let lock = load(&package.join("package.lock"))?;
    require(
        lock["format"] == "oddlang-native-package-lock-v1",
        "package lock format",
    )?;
    let inventory: BTreeMap<_, _> = file_digests(&package.join("src"))?
        .into_iter()
        .map(|(path, digest)| (format!("src/{path}"), digest))
        .collect();
    require(
        lock["generatedFiles"] == json!(inventory),
        "generated source inventory differs from lock",
    )?;
    require(
        lock["manifestSha256"] == sha(manifest)?,
        "manifest lock mismatch",
    )?;
    require(
        lock["cargoManifestSha256"] == sha(&package.join("Cargo.toml"))?,
        "Cargo.toml lock mismatch",
    )?;
    let cargo = package.join("Cargo.lock");
    require(
        lock["cargoLockSha256"] == spec["cargoLock"]["sha256"]
            && lock["cargoLockSha256"] == sha(&cargo)?,
        "Cargo.lock mismatch",
    )?;
    let deps: toml::Table = fs::read_to_string(cargo)?.parse()?;
    for dep in deps
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("expected locked packages")?
    {
        let name = dep
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or("expected dependency name")?;
        if spec["name"] != name {
            let registry = dep.get("source").and_then(toml::Value::as_str);
            let checksum = dep
                .get("checksum")
                .and_then(toml::Value::as_str)
                .unwrap_or("");
            require(
                registry == Some("registry+https://github.com/rust-lang/crates.io-index")
                    && checksum.len() == 64
                    && checksum
                        .bytes()
                        .all(|b| matches!(b,b'0'..=b'9'|b'a'..=b'f')),
                format!("dependency is not pinned in the registry: {name}"),
            )?;
        }
    }
    let components = list(&spec["components"])?;
    let pins = list(&lock["components"])?;
    require(components.len() == pins.len(), "component lock count")?;
    let mut groups = BTreeMap::new();
    for (i, (component, pin)) in components.iter().zip(pins).enumerate() {
        for field in ["kind", "module", "reexports"] {
            require(
                component.get(field).ok_or("missing component field")? == &pin[field],
                format!("component {i} {field} mismatch"),
            )?;
        }
        for field in ["graph", "overlay"] {
            require(
                text(&component[field]["sha256"])? == text(&pin[format!("{field}Sha256")])?,
                format!("component {i} {field} digest mismatch"),
            )?;
        }
        let group = groups.entry(text(&pin["module"])?).or_insert(i);
        let source = package.join(format!("src/group_{group}/__projection_{i}.rs"));
        require(
            pin["sourceSha256"] == sha(&source)?,
            format!("component {i} source digest mismatch"),
        )?;
    }
    Ok(())
}

type Observations = BTreeMap<(String, String), serde_json::Value>;

fn observations(bytes: &[u8]) -> Result<Observations> {
    let mut rows = Observations::new();
    for line in std::str::from_utf8(bytes)?.lines() {
        let (family, name, value): (String, String, serde_json::Value) =
            serde_json::from_str(line)?;
        if rows.insert((family.clone(), name.clone()), value).is_some() {
            return Err(format!("duplicate observation {family}/{name}").into());
        }
    }
    Ok(rows)
}

fn compare(original: &Path, generated: &Path) -> Result<BTreeMap<String, usize>> {
    let reference = fs::read(original)?;
    let candidate = fs::read(generated)?;
    let rows = observations(&reference)?;
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../core-package/expected-observations.json"
    ))?;
    let mut counts = BTreeMap::<&str, usize>::new();
    for (family, _) in rows.keys() {
        *counts.entry(family).or_default() += 1;
    }
    require(
        json!(counts) == contract["counts"],
        "different differential observation coverage",
    )?;
    for check in list(&contract["observations"])? {
        let (family, name, fields): (String, String, serde_json::Map<String, serde_json::Value>) =
            serde_json::from_value(check.clone())?;
        let key = (family, name);
        let observed = rows
            .get(&key)
            .ok_or_else(|| format!("missing expected observation {key:?}"))?;
        require(
            fields.iter().all(|(k, v)| observed.get(k) == Some(v)),
            format!("original outcome differs from independent expectation: {key:?}"),
        )?;
    }
    if candidate != reference {
        let actual = observations(&candidate)?;
        require(
            rows.keys().eq(actual.keys()),
            "different differential observation coverage",
        )?;
        for (key, value) in &rows {
            require(
                value == &actual[key],
                format!("native differential mismatch at {key:?}"),
            )?;
        }
        return Err("native transcripts differ in bytes".into());
    }
    Ok(counts.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn main() -> Result<()> {
    match Args::parse().action {
        Action::Linux(args) => linux::qualify(args),
        Action::Macos(args) => macos::qualify(args),
        Action::MacosEnvironment { work, host } => macos::environment(&work, &host),
        Action::MacosPolicy { work, read, deny } => {
            require(cfg!(target_os = "macos"), "macOS Seatbelt required")?;
            println!(
                "{}",
                public_data_qualification::assembly::seatbelt_policy(
                    &work,
                    &read.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
                    &deny.iter().map(PathBuf::as_path).collect::<Vec<_>>()
                )?
            );
            Ok(())
        }
        Action::ProbeLoopback { port } => linux::probe_loopback(port),
        Action::Compare {
            original,
            generated,
        } => {
            println!(
                "{}",
                serde_json::to_string(&compare(&original, &generated)?)?
            );
            Ok(())
        }
        Action::Observe {
            transcript,
            family,
            name,
        } => {
            let rows = observations(&fs::read(transcript)?)?;
            println!(
                "{}",
                rows.get(&(family, name)).ok_or("missing observation")?
            );
            Ok(())
        }
        Action::Stage {
            portable,
            destination,
            mutation,
        } => stage(&portable, &destination, mutation),
        Action::Verify { package, manifest } => verify(&package, &manifest),
    }
}
