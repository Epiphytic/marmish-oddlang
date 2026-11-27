//! Stage the core package graph and verify its generated Rust source inventory.
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, cli_projection::input_bytes, list, load, product::file_digests, require, sha, text,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Stage {
        portable: PathBuf,
        destination: PathBuf,
    },
    Verify {
        package: PathBuf,
        manifest: PathBuf,
    },
}

fn stage(portable: &Path, destination: &Path) -> Result<()> {
    let root = portable.canonicalize()?;
    fs::create_dir(destination)?;
    let manifest = destination.join("core-package.json");
    fs::copy(root.join("core-package.json"), &manifest)?;
    let spec = load(&manifest)?;
    let copy = |pin: &serde_json::Value| -> Result<()> {
        let relative = Path::new(text(&pin["path"])?);
        require(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "unsafe input path",
        )?;
        let (_, bytes) = input_bytes(&root, pin)?;
        let out = destination.join(relative);
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

fn main() -> Result<()> {
    match Args::parse().action {
        Action::Stage {
            portable,
            destination,
        } => stage(&portable, &destination),
        Action::Verify { package, manifest } => verify(&package, &manifest),
    }
}
