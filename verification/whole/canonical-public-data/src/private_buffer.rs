//! Project private reader packages after checking the portable inputs and excluding source.
use crate::{
    assembly::{Sandbox, root},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const MANIFEST: &str = "prototype-buffer-package.json";

fn sources(base: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, String>) -> Result {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            sources(base, &p, files)?;
        } else {
            require(
                entry.file_type()?.is_file(),
                "unexpected generated filesystem entry",
            )?;
            files.insert(p.strip_prefix(base)?.into(), sha(&p)?);
        }
    }
    Ok(())
}
fn verify_lock(product: &Path, manifest: &Path) -> Result<String> {
    let entries = fs::read_dir(product)?
        .map(|e| {
            Ok(e?
                .file_name()
                .into_string()
                .map_err(|_| "invalid package filename")?)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    require(
        entries
            == ["src", "Cargo.toml", "Cargo.lock", "package.lock"]
                .map(str::to_owned)
                .into(),
        "unexpected package root entries",
    )?;
    let spec = load(manifest)?;
    let lockpath = product.join("package.lock");
    let lock = load(&lockpath)?;
    let mut files = BTreeMap::new();
    sources(product, &product.join("src"), &mut files)?;
    require(
        lock["format"] == "oddlang-native-package-lock-v1"
            && lock["generatedFiles"] == serde_json::json!(files),
        "generated source inventory or lock format changed",
    )?;
    require(
        lock["manifestSha256"] == sha(manifest)?
            && lock["cargoManifestSha256"] == sha(&product.join("Cargo.toml"))?,
        "package or Cargo manifest seal mismatch",
    )?;
    let cargo = product.join("Cargo.lock");
    require(
        lock["cargoLockSha256"] == spec["cargoLock"]["sha256"]
            && lock["cargoLockSha256"] == sha(&cargo)?,
        "Cargo.lock seal mismatch",
    )?;
    let cargo: toml::Value = fs::read_to_string(cargo)?.parse()?;
    for dep in cargo["package"]
        .as_array()
        .ok_or("missing Cargo packages")?
    {
        if dep["name"].as_str() == spec["name"].as_str() {
            continue;
        }
        let checksum = dep
            .get("checksum")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        require(
            dep.get("source").and_then(toml::Value::as_str)
                == Some("registry+https://github.com/rust-lang/crates.io-index")
                && checksum.len() == 64
                && checksum
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "unsealed external dependency",
        )?;
    }
    let components = list(&spec["components"])?;
    let seals = list(&lock["components"])?;
    require(components.len() == seals.len(), "component count changed")?;
    let mut groups = BTreeMap::new();
    for (ordinal, (component, seal)) in components.iter().zip(seals).enumerate() {
        for name in ["kind", "module", "reexports"] {
            require(
                component.get(name).is_some() && component[name] == seal[name],
                format!("component {ordinal} {name} changed"),
            )?;
        }
        for (key, digest) in [("graph", "graphSha256"), ("overlay", "overlaySha256")] {
            require(
                component[key]["sha256"] == seal[digest],
                format!("component {ordinal} {key} seal changed"),
            )?;
        }
        let group = groups.entry(text(&seal["module"])?).or_insert(ordinal);
        let p = product.join(format!("src/group_{group}/__projection_{ordinal}.rs"));
        require(
            sha(&p)? == seal["sourceSha256"],
            format!("component {ordinal} source drift"),
        )?;
    }
    sha(&lockpath)
}
pub fn project(
    work: &Path,
    inputs: &Path,
    cli: &Path,
    original: &Path,
    framework: &Path,
    host: &str,
    denied: &[PathBuf],
) -> Result<(PathBuf, String)> {
    let compiler = work.join("oddlang");
    fs::copy(cli, &compiler)?;
    // This package resolves all external dependencies from the pinned registry.
    let empty = work.join("empty-providers");
    fs::create_dir(&empty)?;
    let sandbox = Sandbox::new(host, work, &[root(), original, framework], &empty)?;
    let manifest = inputs.join(MANIFEST);
    require(
        run(sandbox.command(work)?.arg("/bin/cat").arg(&manifest))? == fs::read(&manifest)?,
        "isolated build cannot read its staged manifest",
    )?;
    let mut hidden = vec![
        whole().join("private-buffer/observations.rs"),
        original.join(buffered::SOURCE),
        framework.join("src/lib.rs"),
    ];
    hidden.extend_from_slice(denied);
    sandbox.deny(&hidden)?;
    let product = work.join("product");
    run(sandbox
        .command(work)?
        .arg(compiler)
        .args(["build", "native-package", "rust"])
        .arg(&manifest)
        .arg(&product))?;
    let lock = verify_lock(&product, &manifest)?;
    run(sandbox
        .command(work)?
        .args([
            "cargo",
            "build",
            "--release",
            "--offline",
            "--locked",
            "--manifest-path",
        ])
        .arg(product.join("Cargo.toml")))?;
    Ok((product, lock))
}

/// Run the historical framework's rustc workspace wrapper on the selected prototype crate.
pub fn capture(framework: &Path, checkout: &Path, host: &str) -> Result<Vec<u8>> {
    use crate::assembly::COMPILER;
    use std::{env, process::Command};
    let tool = |name: &str| -> Result<PathBuf> {
        Ok(
            stdout(Command::new("rustup").args(["which", "--toolchain", "1.97.1", name]))?
                .trim()
                .into(),
        )
    };
    let rustc = tool("rustc")?;
    let cargo = tool("cargo")?;
    let version = stdout(Command::new(&rustc).arg("-vV"))?;
    require(
        version
            .lines()
            .any(|l| l == format!("commit-hash: {COMPILER}"))
            && version.lines().any(|l| l == format!("host: {host}")),
        "unexpected compiler identity",
    )?;
    let tmp = tempfile::tempdir()?;
    let output = tmp.path().canonicalize()?;
    let new_command = || {
        let mut cmd = Command::new(&cargo);
        for (k, _) in env::vars_os() {
            let s = k.to_string_lossy();
            if matches!(
                s.as_ref(),
                "RUSTFLAGS"
                    | "RUSTDOCFLAGS"
                    | "RUSTC_BOOTSTRAP"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "CARGO_ENCODED_RUSTFLAGS"
                    | "CARGO_ENCODED_RUSTDOCFLAGS"
                    | "CARGO_BUILD_TARGET"
                    | "CARGO_BUILD_RUSTFLAGS"
                    | "CARGO_BUILD_RUSTDOCFLAGS"
                    | "CARGO_BUILD_RUSTC_WRAPPER"
                    | "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"
            ) || s.starts_with("CARGO_PROFILE_")
                || (s.starts_with("CARGO_TARGET_")
                    && (s.ends_with("_RUSTFLAGS") || s.ends_with("_RUSTDOCFLAGS")))
            {
                cmd.env_remove(k);
            }
        }
        cmd.env("RUSTC", &rustc).env("RUSTUP_TOOLCHAIN", "1.97.1");
        cmd
    };
    let target = output.join("driver");
    run(new_command()
        .args([
            "build",
            "--quiet",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(framework.join("verification/rust-recovery/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .args(["--target", host])
        .env("RUSTFLAGS", "-C rpath=yes")
        .env("RUSTC_BOOTSTRAP", "oddlang_rust_recovery_driver"))?;
    let manifest = checkout.join("crates/marmish-prototype/Cargo.toml");
    let inventory = output.join("inventory.json");
    let sysroot = stdout(Command::new(&rustc).args(["--print", "sysroot"]))?;
    run(new_command()
        .current_dir(manifest.parent().unwrap())
        .args([
            "check",
            "--quiet",
            "--locked",
            "--offline",
            "--release",
            "--manifest-path",
        ])
        .arg(manifest)
        .arg("--target-dir")
        .arg(output.join("target"))
        .args(["--target", host, "--package", "marmish-prototype", "--lib"])
        .env(
            "RUSTC_WORKSPACE_WRAPPER",
            target.join(host).join("debug/oddlang-rust-recovery-driver"),
        )
        .env("ODD_RECOVERY_PACKAGE", "marmish-prototype")
        .env("ODD_RECOVERY_TARGET", "marmish_prototype")
        .env("ODD_RECOVERY_KIND", "lib")
        .env("ODD_RECOVERY_OUT", &inventory)
        .env("ODD_RECOVERY_SYSROOT", sysroot.trim())
        .env("ODD_RECOVERY_COMPILER", COMPILER)
        .env("ODD_RECOVERY_DATA_SHAPES", "1"))?;
    Ok(fs::read(inventory)?)
}
