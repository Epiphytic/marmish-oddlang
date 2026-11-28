//! Link the retained public-data build and compare the immediate terminal roles.
use super::{ROLES, classify, linked_binaries, replay};
use public_data_qualification::{
    Result,
    artifacts::{cargo_rows, library_closure},
    assembly::{COMPILER, DARWIN, LINUX, Sandbox, bytes_sha, closure_digest, root},
    json_bytes, load, portable,
    product::{pinned, public_product},
    require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const RELEASE: &str = "workspace-v7-canonical-public-data";
const BASE: &str = "8f945d725dea3f4d4812eb6ea24bc7a1aff8d75b";
const PREFIX: &str = "946850f3242203e5901f6fc14ff143adc67b5b55";

#[derive(clap::Args)]
pub struct Args {
    sealed_build: PathBuf,
    original_bin_dir: PathBuf,
    receipt: PathBuf,
}

fn hashes(dir: &Path) -> Result<Value> {
    let mut result = json!({});
    for name in ROLES {
        result[name] = sha(&dir.join(name))?.into();
    }
    Ok(result)
}

fn library_hashes(dir: &Path) -> Result<Value> {
    Ok(json!({"engine":sha(&dir.join("libmarmish_engine.rlib"))?,
              "native":sha(&dir.join("libmarmish_native.rlib"))?}))
}

impl Args {
    pub fn run(self) -> Result {
        let work = self.sealed_build.canonicalize()?;
        let original = self.original_bin_dir.canonicalize()?;
        let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
        let host = match (std::env::consts::ARCH, std::env::consts::OS) {
            ("aarch64", "macos") => DARWIN,
            ("aarch64", "linux") => LINUX,
            _ => return Err("requires a native ARM64 host".into()),
        };
        require(
            version.lines().any(|v| v == format!("host: {host}"))
                && version
                    .lines()
                    .any(|v| v == format!("commit-hash: {COMPILER}")),
            "wrong terminal Rust toolchain",
        )?;
        let app = stdout(
            Command::new("git")
                .current_dir(root())
                .args(["rev-parse", "HEAD"]),
        )?;
        let base_file = whole().join(format!("canonical-public-data/{host}.json"));
        let terminal_file = whole().join(format!("terminal-prefix/{host}.json"));
        pinned(&base_file, BASE)?;
        pinned(&terminal_file, PREFIX)?;
        let base = load(&base_file)?;
        let archived = load(&terminal_file)?;
        classify(&archived)?;
        let originals = hashes(&original)?;
        for name in ROLES {
            require(
                originals[name] == archived["roles"][name]["originalSha256"],
                "wrong pinned original executable",
            )?;
        }
        let product = work.join("candidate");
        let target = work.join("target");
        let release = target.join(host).join("release");
        let libs = library_hashes(&release)?;
        for (name, key) in [
            ("engine", "candidateEngineLibrarySha256"),
            ("native", "candidateNativeLibrarySha256"),
        ] {
            require(
                libs[name] == base["publicData"][key],
                "wrong qualified generated library",
            )?;
        }
        let log = fs::read(work.join("cargo-artifacts.jsonl"))?;
        require(
            bytes_sha(&log) == base["artifactLogSha256"],
            "wrong sealed Cargo log",
        )?;
        let rows = cargo_rows(&log)?;
        let files = library_closure(&rows, &target, host)?;
        let digest = closure_digest(&files)?;
        require(
            base["cargoLibraryClosureEntries"] == files.len()
                && base["cargoLibraryClosureSha256"] == digest,
            "wrong sealed library closure",
        )?;
        let lock = text(&base["workspace"]["candidateWorkspaceLockSha256"])?;
        let verify_product = || public_product(&product, RELEASE, text(&base["companion"])?, lock);
        verify_product()?;
        let sandbox = Sandbox::retained(host, &work, &[root()], &work.join("providers"))?;
        sandbox.deny(&[portable().join(format!("{RELEASE}.json"))])?;
        let mut build = sandbox.command(&product)?;
        let mode = if host == DARWIN {
            replay::Sandbox::Seatbelt
        } else if build.get_program() == "sudo" {
            replay::Sandbox::SudoBwrap
        } else {
            replay::Sandbox::Bwrap
        };
        let raw = run(build
            .args([
                "cargo",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(&target)
            .args(["--message-format=json", "--workspace", "--lib", "--bins"]))?;
        linked_binaries(&product, &target, host, &raw)?;
        require(
            library_closure(&cargo_rows(&raw)?, &target, host)? == files,
            "binary link changed library closure",
        )?;
        let bins = hashes(&release)?;
        let scratch = tempfile::tempdir()?;
        let roles = replay::Args {
            sandbox: mode,
            originals: original.clone(),
            binaries: release.clone(),
            scratch: scratch.path().into(),
        }
        .run(&archived)?;
        verify_product()?;
        require(
            hashes(&original)? == originals
                && hashes(&release)? == bins
                && library_hashes(&release)? == libs
                && library_closure(&rows, &target, host)? == files,
            "terminal artifacts changed during replay",
        )?;
        let report = json!({
            "format": "marmish-combined-terminal-help-version-v1",
            "host": host, "rustc": version.lines().collect::<Vec<_>>(),
            "companion": app.trim(), "framework": base["framework"], "original": archived["original"],
            "workspaceLockSha256": lock, "binaryBuildArtifactLogSha256": bytes_sha(&raw),
            "cargoLibraryClosureSha256": digest, "cargoLibraryClosureEntries": files.len(),
            "sourceWorkspaceReceiptSha256": sha(&base_file)?, "terminalPrefixReceiptSha256": sha(&terminal_file)?,
            "originalBinaries": originals, "candidateBinaries": bins,
            "candidateLibraries": libs, "roles": roles,
        });
        let mut out = tempfile::NamedTempFile::new_in(
            self.receipt
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        out.write_all(&json_bytes(&report)?)?;
        out.persist(self.receipt)?;
        Ok(())
    }
}
