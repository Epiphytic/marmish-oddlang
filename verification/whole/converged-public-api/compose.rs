//! Compose the two public-API releases and their disjoint seven-member convergence.
use crate::{assembly::bytes_sha, *};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Clone, Copy, ValueEnum)]
pub enum Release {
    PrototypeConstant,
    EnginePublicError,
    Converged,
    ReleaseManifest,
    ConvergedDiagnosticsCli,
    CanonicalOffline,
    CanonicalEngineChoices,
    CanonicalRangeData,
    CanonicalRecoveredRange,
    CanonicalMdkTestSeam,
    CanonicalRuntimeStatus,
    ObservationalRecords,
    RangeNativeIdioms,
}
const BASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json";
const LIMIT: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit.json";
pub const ERRORS: &str = "workspace-v7-engine-public-error-identity.json";
const MERGED: &str = "workspace-v7-converged-public-api.json";

pub(crate) struct Descriptor {
    name: String,
    bytes: Vec<u8>,
}
impl Descriptor {
    pub(crate) fn new(name: &'static str, value: &Value) -> Result<Self> {
        Ok(Self {
            name: name.into(),
            bytes: json_bytes(value)?,
        })
    }
    pub(crate) fn from_bytes(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            bytes,
        }
    }
    pub(crate) fn pin(&self) -> Value {
        json!({"path":self.name,"sha256":bytes_sha(&self.bytes)})
    }
}
pub(crate) fn checked_path(root: &Path, descriptor: &Value) -> Result<std::path::PathBuf> {
    let name = text(&descriptor["path"])?;
    require(
        pin(root, name)? == *descriptor,
        format!("input pin differs: {name}"),
    )?;
    Ok(root.join(name))
}
pub(crate) fn canonical_member(root: &Path, name: &str) -> Result<(Value, Value)> {
    let workspace = load(&root.join("workspace-v7-canonical-offline.json"))?;
    let package = load(&checked_path(
        root,
        &workspace["members"][name]["manifest"],
    )?)?;
    Ok((workspace, package))
}
fn components(package: &mut Value) -> Result<&mut Vec<Value>> {
    Ok(package
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("expected package components")?)
}
pub(crate) fn set_member(workspace: &mut Value, member: &str, descriptor: &Descriptor) -> Result {
    let row = workspace
        .get_mut("members")
        .and_then(|rows| rows.get_mut(member))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("missing workspace member {member}"))?;
    row.insert("manifest".into(), descriptor.pin());
    Ok(())
}
fn constant(root: &Path, mut workspace: Value) -> Result<Vec<Descriptor>> {
    let mut prototype = load(&root.join("prototype-buffer-package.json"))?;
    components(&mut prototype)?.push(json!({
        "kind":"shared-declarations","module":"limits","reexports":["MAX_LINE"],
        "graph":pin(root,"prototype-constant-v1/graph.json")?,
        "overlay":pin(root,"prototype-constant-v1/rust.json")?
    }));
    let prototype = Descriptor::new("prototype-buffer-limit-package.json", &prototype)?;
    set_member(&mut workspace, "marmish-prototype", &prototype)?;
    Ok(vec![prototype, Descriptor::new(LIMIT, &workspace)?])
}
fn errors(root: &Path, mut workspace: Value) -> Result<Vec<Descriptor>> {
    let parent =
        |member: &str| load(&root.join(text(&workspace["members"][member]["manifest"]["path"])?));
    let mut engine = parent("marmish-engine")?;
    let mut native = parent("marmish-native")?;
    let [startup] = components(&mut engine)?.as_mut_slice() else {
        return Err("expected one engine startup component".into());
    };
    let startup = startup
        .as_object_mut()
        .ok_or("expected engine startup descriptor")?;
    startup.insert("reexports".into(), json!(["initialize_process_limits"]));
    startup.insert(
        "overlay".into(),
        pin(root, "engine-public-error-v1/process-rust.json")?,
    );
    components(&mut engine)?.push(json!({
        "kind":"finite-error-domain","module":"model",
        "reexports":["EngineError","NotFoundReason","NotExecutableReason","ResolutionReason","Result"],
        "graph":pin(root,"engine-public-error-v1/graph.json")?,
        "overlay":pin(root,"engine-public-error-v1/rust.json")?
    }));
    let backend = load(&root.join("native-backend-choice-package.json"))?;
    let [choice] = list(&backend["components"])?.as_slice() else {
        return Err("expected one backend choice component".into());
    };
    components(&mut native)?.push(choice.clone());
    let engine = Descriptor::new("engine-public-error-package.json", &engine)?;
    let native = Descriptor::new("native-engine-error-identity-package.json", &native)?;
    set_member(&mut workspace, "marmish-engine", &engine)?;
    set_member(&mut workspace, "marmish-native", &native)?;
    Ok(vec![engine, native, Descriptor::new(ERRORS, &workspace)?])
}
fn converge(root: &Path, mut base: Value) -> Result<Vec<Descriptor>> {
    let mut merged = base["members"]
        .as_object()
        .ok_or("missing workspace members")?
        .clone();
    for (release, expected) in [
        (ERRORS, &["marmish-engine", "marmish-native"][..]),
        (LIMIT, &["marmish-prototype"][..]),
    ] {
        let mut parent = load(&root.join(release))?;
        let slot = parent
            .get_mut("members")
            .ok_or("missing parent workspace members")?;
        let rows = std::mem::replace(slot, base["members"].clone());
        require(
            parent == base,
            format!("changed shared workspace fields in {release}"),
        )?;
        let rows = rows.as_object().ok_or("missing parent workspace members")?;
        require(
            rows.keys().collect::<BTreeSet<_>>() == merged.keys().collect(),
            format!("changed workspace membership in {release}"),
        )?;
        let changed: BTreeSet<_> = rows
            .iter()
            .filter(|(name, row)| base["members"][*name] != **row)
            .map(|(name, _)| name.as_str())
            .collect();
        require(
            changed == expected.iter().copied().collect(),
            format!("unexpected member changes in {release}: {changed:?}"),
        )?;
        for name in changed {
            merged.insert(name.to_owned(), rows[name].clone());
        }
    }
    base["members"] = Value::Object(merged);
    Ok(vec![Descriptor::new(MERGED, &base)?])
}
pub fn compose(release: Release, root: &Path, check: bool) -> Result {
    let base = || load(&root.join(BASE));
    let outputs = match release {
        Release::ConvergedDiagnosticsCli => offline_composition::diagnostics(root)?,
        Release::CanonicalOffline => offline_composition::canonical(root)?,
        Release::CanonicalMdkTestSeam => mdk_seam::descriptors(root)?,
        Release::CanonicalRuntimeStatus => runtime_status_composition::descriptors(root)?,
        Release::ObservationalRecords => observational_records::descriptors(root)?,
        Release::RangeNativeIdioms => range_binding::descriptors(root)?,
        Release::ReleaseManifest => manifest_product::descriptors(root)?,
        Release::PrototypeConstant => constant(root, base()?)?,
        Release::EnginePublicError => errors(root, base()?)?,
        Release::Converged => converge(root, base()?)?,
        Release::CanonicalEngineChoices => canonical_composition::choices(root)?,
        Release::CanonicalRangeData => canonical_composition::range(root)?,
        Release::CanonicalRecoveredRange => canonical_composition::recovered(root)?,
    };
    publish(root, outputs, check)
}
pub(crate) fn publish(root: &Path, outputs: Vec<Descriptor>, check: bool) -> Result {
    for descriptor in outputs {
        let path = root.join(descriptor.name);
        if check {
            require(
                fs::read(&path)? == descriptor.bytes,
                format!("stale composed descriptor {}", path.display()),
            )?;
        } else {
            fs::create_dir_all(path.parent().ok_or("missing descriptor directory")?)?;
            fs::write(path, descriptor.bytes)?;
        }
    }
    Ok(())
}
