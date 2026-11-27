//! Check the generated diagnostics release and replay the independent native caller.
use public_data_qualification::{
    Result, assembly::ORIGINAL, load, portable, product::check_members, require, run, sha, whole,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

#[path = "../public-diagnostics/verify.rs"]
#[allow(dead_code)]
mod caller;

pub const RELEASE: &str = "workspace-v7-offline-cli-diagnostics";
pub const BASE: &str = "workspace-v7-offline-cli";
pub const MEMBER: &str = "marmish-native";
pub const SOURCE: &str = "crates/marmish-native/src/diagnostics.rs";
pub const PROBE: &str = "public-diagnostics/probe.rs";

pub fn check(product: &Path) -> Result<Value> {
    let emitted = fs::read(product.join("workspace.lock"))?;
    require(
        emitted == fs::read(portable().join(RELEASE).join("workspace.lock"))?,
        "workspace lock differs from the release pin",
    )?;
    let current: Value = serde_json::from_slice(&emitted)?;
    let parent = load(&portable().join(BASE).join("workspace.lock"))?;
    let mut before = parent["members"]
        .as_object()
        .ok_or("missing parent members")?
        .clone();
    let mut after = current["members"]
        .as_object()
        .ok_or("missing emitted members")?
        .clone();
    let earlier = before
        .shift_remove(MEMBER)
        .ok_or("missing parent native member")?;
    let later = after
        .shift_remove(MEMBER)
        .ok_or("missing generated native member")?;
    require(
        before == after
            && earlier != later
            && parent["cargoLockSha256"] == current["cargoLockSha256"],
        "unexpected workspace changes",
    )?;
    let old_lock = whole().join("generated-diagnostics/parent-package.lock");
    require(
        sha(&old_lock)? == earlier["standalonePackageLockSha256"],
        "incorrect parent native package lock",
    )?;
    let old = load(&old_lock)?;
    let new = load(&product.join(MEMBER).join("package.lock"))?;
    let files = |v: &Value| -> Result<BTreeSet<String>> {
        Ok(v["generatedFiles"]
            .as_object()
            .ok_or("missing generated files")?
            .keys()
            .cloned()
            .collect())
    };
    let names = files(&old)?;
    let added = files(&new)?;
    let changed: BTreeSet<_> = names
        .union(&added)
        .filter(|k| old["generatedFiles"][*k] != new["generatedFiles"][*k])
        .map(String::as_str)
        .collect();
    require(
        changed
            == BTreeSet::from([
                "src/lib.rs",
                "src/group_9/mod.rs",
                "src/group_9/__projection_9.rs",
            ]),
        "unexpected changed native source files",
    )?;
    check_members(product, &current, false)?;
    Ok(current)
}

pub fn observe(
    product: &Path,
    original: &Path,
    compiler: &Path,
    scratch: &Path,
    host: &str,
) -> Result<Value> {
    let source = original.join(SOURCE);
    let reference = load(
        &whole()
            .join("public-diagnostics")
            .join(format!("{host}.json")),
    )?;
    require(
        reference["original"] == ORIGINAL
            && reference["sourceSha256"] == sha(&source)?
            && reference["probeSha256"] == sha(&whole().join(PROBE))?,
        "unexpected upstream diagnostic oracle",
    )?;
    let recovered = scratch.join("recovered.json");
    run(Command::new(compiler)
        .args(["recover", "structured-diagnostics"])
        .arg(source)
        .arg(&recovered))?;
    require(
        fs::read(&recovered)? == fs::read(portable().join("native-diagnostics-v1/graph.json"))?,
        "recovery differs from pinned graph",
    )?;
    let library = product.join("target/release/libmarmish_native.rlib");
    let actual = caller::observe(&library, host, "0.3.1", scratch, None)?;
    require(
        actual == reference["observations"],
        "generated diagnostics differ from original",
    )?;
    Ok(
        json!({"consumerSha256":reference["probeSha256"],"graphSha256":sha(&recovered)?,"originalLibrarySha256":reference["librarySha256"],"librarySha256":sha(&library)?,"observations":actual}),
    )
}
