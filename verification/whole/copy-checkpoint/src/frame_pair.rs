//! Build baseline and mutated frame packages from sealed ontology inputs.
use super::{build_compiler, stage};
use public_data_qualification::{
    assembly::{ORIGINAL, Sandbox, archive, checkout, root},
    *,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn project(
    sandbox: &Sandbox,
    work: &Path,
    cli: &Path,
    plan: &Path,
    out: &Path,
    denied: &[PathBuf],
) -> Result {
    sandbox.deny(denied)?;
    run(sandbox
        .command(work)?
        .arg(cli)
        .args(["build", "native-package", "rust"])
        .arg(plan)
        .arg(out))?;
    run(sandbox
        .command(out)?
        .args(["cargo", "build", "--locked", "--offline", "--release"]))?;
    Ok(())
}
const CRATE: &str = "crates/marmish-stream";
const SOURCE: &str = "crates/marmish-stream/src/copy_protocol.rs";
pub struct FrameReplay {
    pub original: PathBuf,
    pub lane: PathBuf,
    pub framework_pin: String,
    pub rust: String,
    pub host: String,
    pub cli: PathBuf,
    pub mutant: PathBuf,
    pub base: PathBuf,
    pub changed: PathBuf,
    framework: PathBuf,
    scratch: PathBuf,
    _private: tempfile::TempDir,
    product: tempfile::TempDir,
}
impl FrameReplay {
    pub fn new(original: &Path, framework: &Path, lane: &str) -> Result<Self> {
        let original = original.canonicalize()?;
        let framework = framework.canonicalize()?;
        let lane = whole().join(lane);
        let receipt = load(&lane.join("reference-aarch64-apple-darwin.json"))?;
        let framework_pin = text(&receipt["framework"])?.to_owned();
        checkout(&original, ORIGINAL, false)?;
        let head = stdout(
            Command::new("git")
                .current_dir(&framework)
                .args(["rev-parse", "HEAD"]),
        )?;
        require(head.trim() == framework_pin, "framework revision differs")?;
        let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?
            .trim_end()
            .to_owned();
        let host = rust
            .lines()
            .find_map(|s| s.strip_prefix("host: "))
            .ok_or("missing host")?
            .to_owned();
        let private = tempfile::tempdir()?;
        let scratch = private.path().canonicalize()?;
        let source = scratch.join("framework");
        archive(&framework, &framework_pin, &source)?;
        let cli = build_compiler(&source, &scratch.join("tools"), "+1.97.1")?;
        let mutant = scratch.join("mutant");
        archive(&original, ORIGINAL, &mutant)?;
        Ok(Self {
            original,
            framework,
            lane,
            framework_pin,
            rust,
            host,
            cli,
            mutant,
            base: scratch.join("baseline-recovery"),
            changed: scratch.join("mutant-recovery"),
            scratch,
            _private: private,
            product: tempfile::tempdir()?,
        })
    }
    pub fn qualify(
        &self,
        manifest_name: &str,
        data: &Path,
        graphs: &[(&str, &Path)],
        name: &str,
        expected: &[String],
        mutated: &[String],
    ) -> Result<(PathBuf, PathBuf)> {
        let work = self.product.path().canonicalize()?;
        let work = work.as_path();
        let canaries = [
            self.original.join(SOURCE),
            self.mutant.join(SOURCE),
            self.lane.join("caller.rs"),
            data.join("evidence.json"),
            self.base.join("evidence.json"),
            self.changed.join("evidence.json"),
            self.framework.join("Cargo.toml"),
            self.scratch.join("framework/Cargo.toml"),
        ];
        let tools = work.join("oddlang");
        fs::copy(&self.cli, &tools)?;
        let providers = work.join("provider-cache");
        fs::create_dir(&providers)?;
        let sandbox = Sandbox::new(
            &self.host,
            work,
            &[
                &root().canonicalize()?,
                &self.original,
                &self.framework,
                &self.scratch,
            ],
            &providers,
        )?;
        let inputs = work.join("inputs");
        let (manifest, mut plan) = stage(&inputs, manifest_name, false)?;
        require(
            !product::file_digests(&inputs)?.keys().any(|p| {
                p.ends_with(".rs")
                    || Path::new(p)
                        .file_name()
                        .is_some_and(|n| n == "evidence.json")
            }),
            "source or evidence in projection inputs",
        )?;
        let generated = work.join("generated");
        let altered = work.join("altered");
        project(&sandbox, work, &tools, &manifest, &generated, &canaries)?;
        require(
            fs::read(generated.join("package.lock"))? == fs::read(data.join("package.lock"))?,
            "baseline package lock differs",
        )?;
        let components = plan["components"]
            .as_array_mut()
            .ok_or("missing components")?;
        for &(relative, graph) in graphs {
            let mut matches = components
                .iter_mut()
                .filter(|c| c["graph"]["path"] == relative);
            let c = matches.next().ok_or("missing graph component")?;
            require(matches.next().is_none(), "ambiguous graph component")?;
            fs::copy(graph, inputs.join(relative))?;
            c["graph"]["sha256"] = json!(sha(graph)?);
        }
        fs::write(&manifest, json_bytes(&plan)?)?;
        project(&sandbox, work, &tools, &manifest, &altered, &canaries)?;
        let caller = fs::read(self.lane.join("caller.rs"))?;
        for (label, package, lock, rows) in [
            (
                "original",
                self.original.join(CRATE),
                self.original.join("Cargo.lock"),
                expected,
            ),
            (
                "original-mutated",
                self.mutant.join(CRATE),
                self.mutant.join("Cargo.lock"),
                mutated,
            ),
            (
                "generated",
                generated.clone(),
                generated.join("Cargo.lock"),
                expected,
            ),
            (
                "generated-mutated",
                altered.clone(),
                altered.join("Cargo.lock"),
                mutated,
            ),
        ] {
            let exe = stream_caller::build(
                &self.scratch.join(label),
                &package,
                &lock,
                &caller,
                name,
                "2021",
            )?;
            let raw = stdout(&mut Command::new(exe))?;
            require(
                raw.lines().eq(rows.iter().map(String::as_str)),
                format!("{label} observations differ"),
            )?;
        }
        Ok((generated, altered))
    }
}
