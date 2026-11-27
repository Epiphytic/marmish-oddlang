//! Compare generated workspace trees and component seals with their independent release pins.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};
const ENGINE: &str = "marmish-engine";
const NATIVE: &str = "marmish-native";
const SIX: &str = "workspace-v7-recovered-vault-engine-journal-mdk";
const FIVE: &str = "workspace-v7-recovered-vault-engine-journal";
const BEFORE: &str = "workspace-v7-canonical-offline";
const AFTER: &str = "workspace-v7-canonical-public-data";
const BASE_REV: &str = "0f9e57396530fa1c11127e3fff7d8d0018514f00";
const BASE_LOCK: &str = "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4";
pub(crate) const PACKAGES: [&str; 7] = [
    "marmish-core",
    "marmish-engine",
    "marmish-mdk",
    "marmish-native",
    "marmish-prototype",
    "marmish-stream",
    "marmish-vault",
];
fn props(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    Ok(v.as_object().ok_or("expected object")?)
}
fn files(root: &Path, path: &Path, out: &mut BTreeMap<String, String>) -> Result {
    let t = fs::symlink_metadata(path)?.file_type();
    if t.is_file() {
        out.insert(
            path.strip_prefix(root)?
                .to_str()
                .ok_or("non-UTF-8 path")?
                .into(),
            sha(path)?,
        );
    } else if t.is_dir() {
        for entry in fs::read_dir(path)? {
            files(root, &entry?.path(), out)?;
        }
    } else {
        return Err(format!("unexpected generated filesystem entry: {}", path.display()).into());
    }
    Ok(())
}
pub fn file_digests(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut entries = BTreeMap::new();
    files(root, root, &mut entries)?;
    Ok(entries)
}
fn checked(p: &Path, hash: &Value) -> Result {
    require(
        sha(p)? == text(hash)?,
        format!("sealed generated input changed: {}", p.display()),
    )
}
pub fn check_members(product: &Path, lock: &Value, full: bool) -> Result {
    checked(
        &product.join("Cargo.toml"),
        &lock["workspaceCargoManifestSha256"],
    )?;
    checked(&product.join("Cargo.lock"), &lock["cargoLockSha256"])?;
    for (name, row) in props(&lock["members"])? {
        let member = product.join(name);
        let p = member.join("package.lock");
        checked(
            &member.join("Cargo.toml"),
            &row["workspaceCargoManifestSha256"],
        )?;
        checked(&p, &row["standalonePackageLockSha256"])?;
        let mut inventory = BTreeMap::new();
        if full {
            files(&member, &member, &mut inventory)?;
            inventory.remove("Cargo.toml");
            inventory.remove("package.lock");
        } else {
            files(&member, &member.join("src"), &mut inventory)?;
            if member.join("build.rs").exists() {
                files(&member, &member.join("build.rs"), &mut inventory)?;
            }
        }
        require(
            json!(inventory) == load(&p)?["generatedFiles"],
            format!("sealed generated source differs: {name}"),
        )?;
    }
    Ok(())
}
pub fn six(product: &Path) -> Result<Value> {
    let p = product.join("workspace.lock");
    require(
        fs::read(&p)? == fs::read(portable().join(SIX).join("workspace.lock"))?,
        "six-member projection lock mismatch",
    )?;
    let lock = load(&p)?;
    let old = load(&portable().join(FIVE).join("workspace.lock"))?;
    let mut members = props(&lock["members"])?.clone();
    let mdk = members
        .shift_remove("marmish-mdk")
        .ok_or("missing MDK member")?;
    require(
        members.len() == 5
            && members == *props(&old["members"])?
            && mdk["dependencies"] == json!([]),
        "predecessor member locks, edges or isolated MDK member changed",
    )?;
    check_members(product, &lock, false)?;
    let archived = load(&whole().join("mdk-public-data/darwin-arm64/receipt.json"))?;
    require(
        mdk["standalonePackageLockSha256"] == archived["lanes"]["baseline"]["packageLockSha256"],
        "MDK member differs from its standalone qualified projection",
    )?;
    Ok(lock)
}
pub fn pinned(path: &Path, rev: &str) -> Result {
    pinned_at(crate::assembly::root(), path, rev)
}
fn pinned_at(root: &Path, path: &Path, rev: &str) -> Result {
    let name = path.strip_prefix(root)?.to_str().ok_or("non-UTF-8 path")?;
    require(
        fs::read(path)?
            == run(Command::new("git")
                .current_dir(root)
                .args(["show", &format!("{rev}:{name}")]))?,
        format!("release bytes differ from Git pin: {name}"),
    )
}
pub fn public_product(product: &Path, release: &str, rev: &str, hash: &str) -> Result<Value> {
    public_product_at(product, crate::assembly::root(), release, rev, hash)
}
pub fn public_product_at(
    product: &Path,
    inputs: &Path,
    release: &str,
    rev: &str,
    hash: &str,
) -> Result<Value> {
    let portable = inputs.join("oddlang/portable");
    let manifest = portable.join(format!("{release}.json"));
    let reference = portable.join(release).join("workspace.lock");
    for path in [&manifest, &reference] {
        pinned_at(inputs, path, rev)?;
    }
    let p = product.join("workspace.lock");
    require(
        sha(&p)? == hash && fs::read(p)? == fs::read(&reference)?,
        "generated workspace lock differs",
    )?;
    let lock = load(&reference)?;
    checked(&manifest, &lock["inputSha256"])?;
    let manifest = load(&manifest)?;
    let members: BTreeSet<_> = props(&lock["members"])?
        .keys()
        .map(String::as_str)
        .collect();
    require(
        members == PACKAGES.into_iter().collect(),
        "wrong generated workspace member inventory",
    )?;
    let names = fs::read_dir(product)?
        .map(|e| {
            let e = e?;
            require(!e.file_type()?.is_symlink(), "unexpected top-level symlink")?;
            Ok(e.file_name()
                .into_string()
                .map_err(|_| "non-UTF-8 filename")?)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    require(
        names.iter().map(String::as_str).eq(members
            .into_iter()
            .chain(["Cargo.lock", "Cargo.toml", "workspace.lock"])
            .collect::<BTreeSet<_>>()),
        "unexpected generated workspace entries",
    )?;
    check_members(product, &lock, true)?;
    for (name, member) in props(&lock["members"])? {
        let pin = &manifest["members"][name]["manifest"];
        checked(&portable.join(text(&pin["path"])?), &member["inputSha256"])?;
        require(
            pin["sha256"] == member["inputSha256"],
            "workspace manifest member digest mismatch",
        )?;
    }
    Ok(lock)
}
fn delta(before: &Value, after: &Value) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let a = props(&before["generatedFiles"])?;
    let b = props(&after["generatedFiles"])?;
    require(
        a.keys().all(|k| b.contains_key(k)),
        "qualified files disappeared",
    )?;
    Ok((
        a.iter()
            .filter(|(k, v)| b.get(*k) != Some(*v))
            .map(|(k, _)| k.clone())
            .collect(),
        b.keys().filter(|k| !a.contains_key(*k)).cloned().collect(),
    ))
}
pub fn snapshots(base: &Path, product: &Path, revision: &str) -> Result<Value> {
    let candidate_hash = sha(&portable().join(AFTER).join("workspace.lock"))?;
    let before = public_product(base, BEFORE, BASE_REV, BASE_LOCK)?;
    let after = public_product(product, AFTER, revision, &candidate_hash)?;
    for k in ["format", "workspaceCargoManifestSha256"] {
        require(
            after[k] == before[k],
            "root workspace Cargo structure changed",
        )?;
    }
    require(
        fs::read(product.join("Cargo.lock"))?
            == fs::read(portable().join(AFTER).join("Cargo.lock"))?,
        "generated root Cargo lock changed",
    )?;
    for (name, row) in props(&after["members"])? {
        let prev = props(&before["members"][name])?;
        let actual: BTreeSet<_> = props(row)?
            .iter()
            .filter(|(k, v)| prev.get(*k) != Some(*v))
            .map(|(k, _)| k.as_str())
            .collect();
        let fields: &[&str] = match name.as_str() {
            ENGINE => &[
                "inputSha256",
                "standalonePackageLockSha256",
                "workspaceCargoManifestSha256",
            ],
            NATIVE => &["inputSha256", "standalonePackageLockSha256"],
            _ => &[],
        };
        require(
            prev.keys().all(|k| row.get(k).is_some()) && actual == fields.iter().copied().collect(),
            format!("unexpected workspace member delta: {name}"),
        )?;
    }
    let pkg = |root: &Path, name: &str| load(&root.join(name).join("package.lock"));
    let eng0 = pkg(base, ENGINE)?;
    let eng1 = pkg(product, ENGINE)?;
    let choice_path = portable().join("engine-fieldless-choices-v1/package.lock");
    let choices = load(&choice_path)?;
    let workspace = load(&portable().join("workspace-v7-canonical-engine-choices/workspace.lock"))?;
    checked(
        &choice_path,
        &workspace["members"][ENGINE]["standalonePackageLockSha256"],
    )?;
    let reason = load(&portable().join("engine-gap-reason-v1/package.lock"))?;
    let components = list(&eng1["components"])?;
    let (wire, prefix) = components.split_last().ok_or("missing wire component")?;
    require(
        prefix == list(&choices["components"])?
            && prefix.split_last().is_some_and(|(_, initial)| {
                Some(initial) == eng0["components"].as_array().map(Vec::as_slice)
            }),
        "inherited engine projection changed",
    )?;
    for k in ["projection", "sourceSha256"] {
        require(
            wire[k] == reason["components"][0][k],
            "combined missing reason projection differs from verified output",
        )?;
    }
    for (k, file) in [
        ("graphSha256", "graph.json"),
        ("overlaySha256", "rust.json"),
    ] {
        checked(
            &portable().join("engine-gap-reason-recovered-v1").join(file),
            &wire[k],
        )?;
    }
    let (changed, added) = delta(&eng0, &eng1)?;
    let paths = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    require(
        changed == paths(&["src/lib.rs", "src/group_1/mod.rs"])
            && added
                == paths(&[
                    "src/group_1/__projection_3.rs",
                    "src/group_1/__projection_4.rs",
                ]),
        "unexpected generated engine source changes",
    )?;
    for (path, tail) in [
        (
            "src/lib.rs",
            "pub use model::{Stream, SubmitDisposition};\npub use model::{MissingReason};\n",
        ),
        (
            "src/group_1/mod.rs",
            "mod __projection_3;\npub use __projection_3::{Stream, SubmitDisposition};\nmod __projection_4;\npub use __projection_4::{MissingReason};\n",
        ),
    ] {
        let mut expected = fs::read(base.join(ENGINE).join(path))?;
        expected.extend(tail.bytes());
        require(
            fs::read(product.join(ENGINE).join(path))? == expected,
            format!("unexpected engine export structure: {path}"),
        )?;
    }
    let nat0 = pkg(base, NATIVE)?;
    let nat1 = pkg(product, NATIVE)?;
    require(
        after["members"][NATIVE]
            == load(&portable().join("workspace-v7-canonical-runtime-status/workspace.lock"))?["members"]
                [NATIVE],
        "combined native member differs from separately qualified RuntimeStatus",
    )?;
    let mut expected = list(&nat0["components"])?.clone();
    let indices: Vec<_> = expected
        .iter()
        .enumerate()
        .filter(|(_, c)| c["module"] == "record_views")
        .map(|(i, _)| i)
        .collect();
    let [i] = indices.as_slice() else {
        return Err("expected one journal component".into());
    };
    expected[*i] = list(&nat1["components"])?
        .get(*i)
        .ok_or("missing journal component")?
        .clone();
    let suffix = format!("/__projection_{i}.rs");
    let leaves: Vec<_> = props(&nat0["generatedFiles"])?
        .keys()
        .filter(|k| k.ends_with(&suffix))
        .collect();
    let [leaf] = leaves.as_slice() else {
        return Err("expected one journal leaf".into());
    };
    let (changed_native, added_native) = delta(&nat0, &nat1)?;
    require(
        expected == *list(&nat1["components"])?
            && changed_native == paths(&[leaf])
            && added_native.is_empty(),
        "native status should change only its existing generated record leaf",
    )?;
    let unchanged: Vec<_> = PACKAGES
        .into_iter()
        .filter(|p| *p != NATIVE && *p != ENGINE)
        .collect();
    Ok(json!({
        "canonicalWorkspaceLockSha256": BASE_LOCK,
        "candidateWorkspaceLockSha256": candidate_hash,
        "unchangedMemberLocks": unchanged,
        "memberLocks": after["members"],
        "rootCargoLockSha256": after["cargoLockSha256"],
        "rootCargoManifestSha256": after["workspaceCargoManifestSha256"],
        "engineChangedFiles": changed,
        "engineAddedFiles": added,
        "nativeChangedFiles": changed_native,
    }))
}
