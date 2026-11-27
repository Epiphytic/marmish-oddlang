//! Validate the graph-only workspace archive against Cargo metadata and pinned native sources.
mod controls;
mod coverage;
mod publish;
use clap::{Parser, Subcommand};
use public_data_qualification::{
    Result, ascii_json,
    assembly::{bytes_sha, checkout},
    list, load, require, run, sha, text,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const FRAMEWORK: &str = "5ec32f8177cf9d7b4ffcce0c192c5553af7f2042";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
const VOCAB: &str = "oddlang-data-shapes-v1";
const TRIPLES: [&str; 3] = [
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
];
const ADAPTER_FILES: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "driver.rs",
    "shape_model.rs",
    "shapes.rs",
];

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Controls {
        original: PathBuf,
        framework: PathBuf,
        archive: PathBuf,
    },
    Verify {
        original: PathBuf,
        framework: PathBuf,
        archive: PathBuf,
    },
    Seal {
        original: PathBuf,
        framework: PathBuf,
        archive: PathBuf,
        #[arg(required=true,num_args=1..)]
        work: Vec<PathBuf>,
    },
    Compare {
        original: PathBuf,
        framework: PathBuf,
        archive: PathBuf,
        work: PathBuf,
    },
    Recover {
        original: PathBuf,
        framework: PathBuf,
        work: PathBuf,
        triple: String,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
        #[arg(long = "target")]
        targets: Vec<String>,
        #[arg(long,value_parser=["default","fault-injection","installed-checks"])]
        feature_mode: Option<String>,
    },
}
fn sorted<'a>(value: &'a Value, key: &str) -> Result<Vec<&'a Value>> {
    let mut pairs = Vec::new();
    for row in list(value)? {
        pairs.push((text(&row[key])?, row));
    }
    pairs.sort_by_key(|p| p.0);
    Ok(pairs.into_iter().map(|p| p.1).collect())
}
fn matrix(original: &Path) -> Result<(Vec<Value>, Vec<Value>)> {
    checkout(original, ORIGINAL, false)?;
    let metadata: Value =
        serde_json::from_slice(&run(Command::new("cargo").current_dir(original).args([
            "+1.97.1",
            "metadata",
            "--locked",
            "--offline",
            "--format-version=1",
            "--no-deps",
        ]))?)?;
    let mut targets = Vec::new();
    let mut cells = Vec::new();
    for package in sorted(&metadata["packages"], "name")? {
        if !list(&metadata["workspace_members"])?.contains(&package["id"]) {
            continue;
        }
        for target in sorted(&package["targets"], "name")? {
            if target["kind"] != json!(["lib"]) && target["kind"] != json!(["bin"]) {
                continue;
            }
            let mut required: Vec<String> = serde_json::from_value(
                target
                    .get("required-features")
                    .cloned()
                    .unwrap_or(json!([])),
            )?;
            required.sort();
            let mut selected = json!({"package":package["name"],"kind":target["kind"][0],"target":target["name"],"requiredFeatures":required});
            targets.push(selected.clone());
            selected.as_object_mut().unwrap().remove("requiredFeatures");
            selected["defaultFeatures"] = true.into();
            let features = package["features"]
                .as_object()
                .ok_or("expected Cargo features")?;
            let defaults: Vec<String> =
                serde_json::from_value(features.get("default").cloned().unwrap_or(json!([])))?;
            for triple in TRIPLES {
                selected["triple"] = triple.into();
                for opt in [None, Some("fault-injection"), Some("installed-checks")] {
                    if opt.is_some_and(|f| !features.contains_key(f)) {
                        continue;
                    }
                    if required
                        .iter()
                        .all(|r| defaults.contains(r) || Some(r.as_str()) == opt)
                    {
                        selected["features"] = json!(opt.into_iter().collect::<Vec<_>>());
                        cells.push(selected.clone());
                    }
                }
            }
        }
    }
    Ok((targets, cells))
}
fn key(cell: &Value) -> Result<String> {
    let mut parts = Vec::new();
    for k in ["triple", "package", "kind", "target"] {
        parts.push(text(&cell[k])?);
    }
    for f in list(&cell["features"])? {
        parts.push(text(f)?);
    }
    Ok(parts.join("."))
}
fn is_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
struct Pins {
    common: Value,
    manifests: BTreeMap<String, String>,
}
impl Pins {
    fn new(original: &Path, framework: &Path) -> Result<Self> {
        let adapter = framework.join("verification/rust-recovery");
        let mut inputs = Map::new();
        for file in ADAPTER_FILES {
            inputs.insert(file.into(), sha(&adapter.join(file))?.into());
        }
        Ok(Self {
            common: json!({"compilerCommit":COMPILER,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,
            "manifestSha256":sha(&original.join("Cargo.toml"))?,"adapterSha256":sha(&adapter.join("run.sh"))?,
            "adapterInputsSha256":inputs}),
            manifests: BTreeMap::new(),
        })
    }
    fn validate(
        &mut self,
        original: &Path,
        cell: &Value,
        digest: &str,
        lock: &Value,
    ) -> Result<String> {
        let name = text(&cell["package"])?;
        require(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
            "invalid Cargo package name",
        )?;
        if !self.manifests.contains_key(name) {
            self.manifests.insert(
                name.into(),
                sha(&original.join("crates").join(name).join("Cargo.toml"))?,
            );
        }
        let mut expected = self.common.clone();
        for (dest, source) in [
            ("package", "package"),
            ("targetKind", "kind"),
            ("targetName", "target"),
            ("targetTriple", "triple"),
            ("features", "features"),
            ("defaultFeatures", "defaultFeatures"),
        ] {
            expected[dest] = cell[source].clone();
        }
        expected["packageManifestSha256"] = self.manifests[name].clone().into();
        let pin = &lock["pin"];
        require(
            lock == &json!({"vocabulary":VOCAB,"pin":pin,"shapesSha256":digest,"qualification":"candidate"}),
            "bad candidate lock",
        )?;
        require(
            pin["cargoConfigSha256"]
                .as_object()
                .is_some_and(|cfg| cfg.values().all(|v| v.as_str().is_some_and(is_digest))),
            "invalid Cargo configuration",
        )?;
        require(
            pin["sourceRevision"] == ORIGINAL,
            "missing or invalid source revision",
        )?;
        expected["sourceRevision"] = ORIGINAL.into();
        expected["cargoConfigSha256"] = pin["cargoConfigSha256"].clone();
        require(expected == *pin, "profile pin mismatch")?;
        let mut semantic = lock.clone();
        semantic["pin"]
            .as_object_mut()
            .ok_or("missing pin")?
            .remove("cargoConfigSha256");
        semantic.sort_all_objects();
        Ok(bytes_sha(&ascii_json(&serde_json::to_string(&semantic)?)))
    }
}
fn names(path: &Path) -> Result<BTreeSet<String>> {
    fs::read_dir(path)?
        .map(|e| {
            Ok(e?
                .file_name()
                .into_string()
                .map_err(|_| "invalid archive name")?)
        })
        .collect()
}
fn verify(original: &Path, framework: &Path, archive: &Path) -> Result<()> {
    checkout(framework, FRAMEWORK, false)?;
    let mut entries = names(archive)?;
    entries.remove("README.md");
    require(
        entries == ["graphs", "locks", "release.json"].map(String::from).into(),
        "invalid archive root inventory",
    )?;
    let mut release = load(&archive.join("release.json"))?;
    let (targets, cells) = matrix(original)?;
    let fields = release.as_object_mut().ok_or("invalid release")?;
    require(
        fields.remove("targets") == Some(json!(targets)),
        "target inventory differs from Cargo metadata",
    )?;
    let records = fields.remove("cells").ok_or("missing profile cells")?;
    require(
        release
            == json!({"version":1,"upstreamRevision":ORIGINAL,"frameworkRevision":FRAMEWORK,"vocabulary":VOCAB,"compilerCommit":COMPILER,"cargoProfile":"release"}),
        "release pin mismatch",
    )?;
    let records = list(&records)?;
    require(
        records.len() == cells.len(),
        "missing or extra profile cells",
    )?;
    let mut graphs = BTreeSet::new();
    let mut locks = BTreeSet::new();
    let mut pins = Pins::new(original, framework)?;
    for (mut cell, record) in cells.into_iter().zip(records) {
        if record["status"] == "unobserved" {
            cell["status"] = "unobserved".into();
            require(cell == *record, "invalid unobserved cell")?;
            continue;
        }
        let lock_name = format!("{}.json", key(&cell)?);
        let lock_path = archive.join("locks").join(&lock_name);
        let lock_hash = sha(&lock_path)?;
        require(record["lockSha256"] == lock_hash, "lock digest mismatch")?;
        let digest = text(&record["graphSha256"])?;
        require(is_digest(digest), "invalid graph digest")?;
        let graph_name = format!("{digest}.json");
        let graph_path = archive.join("graphs").join(&graph_name);
        require(sha(&graph_path)? == digest, "graph digest mismatch")?;
        let semantic = pins.validate(original, &cell, digest, &load(&lock_path)?)?;
        cell["coverage"] = coverage::coverage(&load(&graph_path)?, text(&cell["target"])?)?;
        cell["status"] = "candidate".into();
        cell["graphSha256"] = digest.into();
        cell["lockSha256"] = lock_hash.into();
        cell["semanticLockSha256"] = semantic.into();
        require(cell == *record, "cell metadata mismatch")?;
        graphs.insert(graph_name);
        locks.insert(lock_name);
    }
    require(!graphs.is_empty(), "archive contains no observations")?;
    require(
        graphs == names(&archive.join("graphs"))?,
        "extra graph files",
    )?;
    require(locks == names(&archive.join("locks"))?, "extra lock files")
}
fn main() -> Result<()> {
    match Args::parse().action {
        Action::Controls {
            original,
            framework,
            archive,
        } => controls::run_controls(&original, &framework, &archive),
        Action::Verify {
            original,
            framework,
            archive,
        } => verify(&original, &framework, &archive),
        Action::Seal {
            original,
            framework,
            archive,
            work,
        } => publish::seal(&original, &framework, &archive, &work),
        Action::Compare {
            original,
            framework,
            archive,
            work,
        } => publish::compare(&original, &framework, &archive, &work),
        Action::Recover {
            original,
            framework,
            work,
            triple,
            jobs,
            targets,
            feature_mode,
        } => publish::recover(
            &original.canonicalize()?,
            &framework.canonicalize()?,
            &work,
            &triple,
            jobs,
            &targets,
            feature_mode.as_deref(),
        ),
    }
}
