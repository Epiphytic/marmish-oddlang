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
fn check_member_files(
    product: &Path,
    lock: &Value,
    inventory: impl Fn(&str, &Path) -> Result<BTreeMap<String, String>>,
) -> Result {
    checked(&product.join("Cargo.lock"), &lock["cargoLockSha256"])?;
    for (name, row) in props(&lock["members"])? {
        require(
            Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name),
            "invalid member name",
        )?;
        let member = product.join(name);
        let p = member.join("package.lock");
        checked(
            &member.join("Cargo.toml"),
            &row["workspaceCargoManifestSha256"],
        )?;
        checked(&p, &row["standalonePackageLockSha256"])?;
        require(
            json!(inventory(name, &member)?) == load(&p)?["generatedFiles"],
            format!("sealed generated source differs: {name}"),
        )?;
    }
    Ok(())
}

fn source_inventory(member: &Path, build_script: bool) -> Result<BTreeMap<String, String>> {
    let mut inventory = BTreeMap::new();
    files(member, &member.join("src"), &mut inventory)?;
    if build_script {
        files(member, &member.join("build.rs"), &mut inventory)?;
    }
    Ok(inventory)
}

pub fn check_members(product: &Path, lock: &Value, full: bool) -> Result {
    checked(
        &product.join("Cargo.toml"),
        &lock["workspaceCargoManifestSha256"],
    )?;
    check_member_files(product, lock, |_, member| {
        if !full {
            return source_inventory(member, member.join("build.rs").exists());
        }
        let mut all = file_digests(member)?;
        all.remove("Cargo.toml");
        all.remove("package.lock");
        Ok(all)
    })
}

fn contains_only_directories(path: &Path) -> Result<bool> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !contains_only_directories(&entry.path())? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Check the sealed product before and after a Cargo build, excluding its target directory.
pub fn export_product(product: &Path, pinned_lock: &Path) -> Result<Value> {
    let emitted = product.join("workspace.lock");
    let bytes = fs::read(pinned_lock)?;
    require(
        emitted.symlink_metadata()?.is_file() && fs::read(&emitted)? == bytes,
        "generated workspace lock differs",
    )?;
    let lock: Value = serde_json::from_slice(&bytes)?;
    let members = props(&lock["members"])?;
    for entry in fs::read_dir(product)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF-8 product path")?;
        if matches!(name, "Cargo.toml" | "Cargo.lock" | "workspace.lock") {
            require(ty.is_file(), "invalid workspace metadata file")?;
        } else if name == "target" || members.contains_key(name) {
            require(ty.is_dir(), "expected ordinary workspace directory")?;
            if members.contains_key(name) {
                for file in ["Cargo.toml", "package.lock"] {
                    require(
                        entry.path().join(file).symlink_metadata()?.is_file(),
                        "invalid member metadata file",
                    )?;
                }
            }
        } else {
            require(
                ty.is_dir() && contains_only_directories(&entry.path())?,
                "unexpected workspace entry",
            )?;
        }
    }
    checked(
        &product.join("Cargo.toml"),
        &lock["workspaceCargoManifestSha256"],
    )?;
    check_member_files(product, &lock, |_, member| {
        let mut all = file_digests(member)?;
        all.remove("Cargo.toml");
        all.remove("package.lock");
        require(
            all.keys().all(|p| p == "build.rs" || p.starts_with("src/")),
            "unexpected member file",
        )?;
        Ok(all)
    })?;
    Ok(lock)
}

/// Older qualification releases checked sources only. Unified releases added a root
/// manifest pin, then report-bearing releases specified the exact member layout.
pub fn verify_release_tree(product: &Path, lock: &Value, report: bool, unified: bool) -> Result {
    if unified {
        checked(
            &product.join("Cargo.toml"),
            &lock["workspaceCargoManifestSha256"],
        )?;
    }
    check_member_files(product, lock, |name, member| {
        let build_script = report && name == NATIVE;
        if report {
            let mut allowed = BTreeSet::from_iter(
                ["Cargo.toml", "package.lock", "src"].map(std::ffi::OsString::from),
            );
            if build_script {
                allowed.insert("build.rs".into());
            }
            let actual: BTreeSet<_> = fs::read_dir(member)?
                .map(|e| e.map(|entry| entry.file_name()))
                .collect::<std::io::Result<_>>()?;
            require(
                actual == allowed,
                format!("unexpected generated package paths: {name}"),
            )?;
        }
        source_inventory(member, build_script)
    })
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
    // Checked inputs may already use the physical path to a symlinked checkout.
    let relative = match path.strip_prefix(root) {
        Ok(name) => name,
        Err(_) => path.strip_prefix(root.canonicalize()?)?,
    };
    let name = relative.to_str().ok_or("non-UTF-8 path")?;
    require(
        fs::read(path)?
            == run(Command::new("git")
                .current_dir(root)
                .args(["show", &format!("{rev}:{name}")]))?,
        format!("release bytes differ from Git pin: {name}"),
    )
}
pub fn public_product(product: &Path, release: &str, rev: &str, hash: &str) -> Result<Value> {
    Ok(public_product_at(product, crate::assembly::root(), release, rev, hash)?.0)
}
pub fn public_product_at(
    product: &Path,
    inputs: &Path,
    release: &str,
    rev: &str,
    hash: &str,
) -> Result<(Value, Value)> {
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
    Ok((lock, manifest))
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
fn check_appended_exports(base: &Path, product: &Path, paths: &[(&str, &str)]) -> Result {
    for &(path, tail) in paths {
        let mut expected = fs::read(base.join(ENGINE).join(path))?;
        expected.extend(tail.bytes());
        require(
            fs::read(product.join(ENGINE).join(path))? == expected,
            format!("unexpected engine export structure: {path}"),
        )?;
    }
    Ok(())
}

pub fn engine_choice_snapshot(
    base: &Path,
    product: &Path,
    revision: &str,
    lock_sha: &str,
) -> Result<Value> {
    let before = public_product(base, BEFORE, BASE_REV, BASE_LOCK)?;
    let after = public_product(
        product,
        "workspace-v7-canonical-engine-choices",
        revision,
        lock_sha,
    )?;
    let old_root = props(&before)?;
    let new_root = props(&after)?;
    require(
        old_root.len() == new_root.len()
            && old_root.iter().all(|(key, old)| {
                new_root.get(key).is_some_and(|new| {
                    matches!(key.as_str(), "inputSha256" | "members") || new == old
                })
            }),
        "root workspace structure changed",
    )?;

    let mut locks = BTreeMap::new();
    for name in PACKAGES {
        let prior = &before["members"][name];
        let next = &after["members"][name];
        let digest = sha(&product.join(name).join("package.lock"))?;
        if name == ENGINE {
            for key in ["dependencies", "workspaceCargoManifestSha256"] {
                require(prior[key] == next[key], format!("engine {key} changed"))?;
            }
        } else {
            require(
                prior == next && digest == sha(&base.join(name).join("package.lock"))?,
                format!("inherited member changed: {name}"),
            )?;
        }
        locks.insert(name, digest);
    }

    let seal = portable().join("engine-fieldless-choices-v1/package.lock");
    pinned(&seal, "042f018252c9ed04bea97e10f7da1df16da1e0b0")?;
    require(
        fs::read(&seal)? == fs::read(product.join(ENGINE).join("package.lock"))?,
        "generated engine differs from its standalone package",
    )?;
    let next = load(&seal)?;
    let prior = load(&base.join(ENGINE).join("package.lock"))?;
    let (choice, inherited) = list(&next["components"])?
        .split_last()
        .ok_or("missing engine choice component")?;
    require(
        inherited == list(&prior["components"])?
            && choice["reexports"] == json!(["Stream", "SubmitDisposition"])
            && next["cargoLockSha256"] == prior["cargoLockSha256"],
        "unexpected engine package change",
    )?;
    let (changed, added) = delta(&prior, &next)?;
    require(
        changed
            .iter()
            .map(String::as_str)
            .eq(["src/group_1/mod.rs", "src/lib.rs"])
            && added
                .iter()
                .map(String::as_str)
                .eq(["src/group_1/__projection_3.rs"]),
        "unexpected engine source changes",
    )?;
    check_appended_exports(
        base,
        product,
        &[
            (
                "src/lib.rs",
                "pub use model::{Stream, SubmitDisposition};\n",
            ),
            (
                "src/group_1/mod.rs",
                "mod __projection_3;\npub use __projection_3::{Stream, SubmitDisposition};\n",
            ),
        ],
    )?;
    Ok(json!({"rootCargoLockSha256":after["cargoLockSha256"],
        "rootCargoManifestSha256":after["workspaceCargoManifestSha256"],"packageLocks":locks,
        "changedEngineSources":changed,"addedEngineSources":added}))
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
    check_appended_exports(
        base,
        product,
        &[
            (
                "src/lib.rs",
                "pub use model::{Stream, SubmitDisposition};\npub use model::{MissingReason};\n",
            ),
            (
                "src/group_1/mod.rs",
                "mod __projection_3;\npub use __projection_3::{Stream, SubmitDisposition};\nmod __projection_4;\npub use __projection_4::{MissingReason};\n",
            ),
        ],
    )?;
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
