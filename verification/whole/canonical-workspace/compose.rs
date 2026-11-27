//! Compose pinned packages by (kind, module) and preserve the ordered component lists.
use crate::{Result, api_composition::Descriptor, assembly::bytes_sha, list, load, require, text};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const PUBLIC: &str = "workspace-v7-converged-public-api.json";
const DIAGNOSTICS: &str = "workspace-v7-converged-diagnostics-cli.json";
const ENGINE: &str = "marmish-engine";
const NATIVE: &str = "marmish-native";
const VAULT: &str = "marmish-vault";

fn object(value: &Value) -> Result<&Map<String, Value>> {
    Ok(value.as_object().ok_or("expected descriptor object")?)
}
fn apart_from(a: &Value, b: &Value, field: &str) -> Result<bool> {
    let a = object(a)?;
    let b = object(b)?;
    Ok(a.keys().filter(|k| k.as_str() != field).count()
        == b.keys().filter(|k| k.as_str() != field).count()
        && a.iter()
            .filter(|(k, _)| k.as_str() != field)
            .all(|(k, v)| b.get(k) == Some(v)))
}
fn checked(root: &Path, descriptor: &Value) -> Result<Value> {
    let relative = text(&descriptor["path"])?;
    require(
        Path::new(relative).file_name().and_then(|s| s.to_str()) == Some(relative),
        "package must be a sibling of the workspace",
    )?;
    let path = root.join(relative).canonicalize()?;
    require(
        path.parent() == Some(root) && path.file_name() == Path::new(relative).file_name(),
        "package path does not match its pin",
    )?;
    let bytes = fs::read(path)?;
    require(
        *descriptor == json!({"path":relative,"sha256":bytes_sha(&bytes)}),
        "package pin differs",
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}
type Key<'a> = (&'a str, &'a str);
fn key(row: &Value) -> Result<Key<'_>> {
    Ok((text(&row["kind"])?, text(&row["module"])?))
}
fn components(package: &Value) -> Result<&[Value]> {
    let rows = list(&package["components"])?;
    let mut seen = BTreeSet::new();
    for row in rows {
        require(seen.insert(key(row)?), "duplicate native component")?;
    }
    Ok(rows)
}
fn combine(base: &Value, donors: &[Value]) -> Result<Value> {
    let parent = components(base)?;
    let inherited = parent
        .iter()
        .map(|row| Ok((key(row)?, row)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut edits = BTreeMap::new();
    for branch in donors {
        require(
            apart_from(base, branch, "components")?,
            "native package metadata differs",
        )?;
        let mut retained = 0;
        for row in components(branch)? {
            let id = key(row)?;
            if let Some(old) = inherited.get(&id) {
                retained += 1;
                if *old == row {
                    continue;
                }
            }
            require(
                edits.insert(id, row).is_none_or(|previous| previous == row),
                format!("incompatible edits to native component {id:?}"),
            )?;
        }
        require(
            retained == inherited.len(),
            "native package lost inherited components",
        )?;
    }
    let mut rows = Vec::new();
    for row in parent {
        rows.push(edits.remove(&key(row)?).unwrap_or(row));
    }
    rows.extend(edits.into_values());
    let mut out = base.clone();
    out["components"] = json!(rows);
    Ok(out)
}
// These releases publish UTF-8 JSON (including non-ASCII literals) with insertion order retained.
fn descriptor(name: impl Into<String>, v: &Value) -> Result<Descriptor> {
    let mut bytes = serde_json::to_vec_pretty(v)?;
    bytes.push(b'\n');
    Ok(Descriptor::from_bytes(name, bytes))
}
fn members(w: &Value) -> Result<&Map<String, Value>> {
    object(&w["members"])
}
fn check_workspace(base: &Value, branch: &Value, allowed: Option<&[&str]>) -> Result {
    let old = members(base)?;
    let new = members(branch)?;
    require(
        apart_from(base, branch, "members")? && old.len() == new.len(),
        "workspace metadata or member count differs",
    )?;
    for (name, row) in old {
        let next = new.get(name).ok_or("missing inherited workspace member")?;
        require(
            apart_from(row, next, "manifest")?
                && (row == next || allowed.is_none_or(|names| names.contains(&name.as_str()))),
            format!("unexpected member or dependency edit: {name}"),
        )?;
    }
    Ok(())
}
pub(crate) fn diagnostics(root: &Path) -> Result<Vec<Descriptor>> {
    let root = root.canonicalize()?;
    let base = load(&root.join("workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"))?;
    let mut branches = Vec::new();
    for file in [
        PUBLIC,
        "workspace-v7-offline-cli.json",
        "workspace-v7-offline-cli-diagnostics.json",
    ] {
        let branch = load(&root.join(file))?;
        check_workspace(&base, &branch, None)?;
        branches.push(branch);
    }
    let mut out = base.clone();
    let mut files = Vec::new();
    for (name, member) in members(&base)? {
        let variants: Vec<_> = branches
            .iter()
            .map(|b| &b["members"][name])
            .filter(|row| *row != member)
            .collect();
        if variants.is_empty() {
            continue;
        }
        let donors = variants
            .iter()
            .map(|row| checked(&root, &row["manifest"]))
            .collect::<Result<Vec<_>>>()?;
        let merged = combine(&checked(&root, &member["manifest"])?, &donors)?;
        if let Some(i) = donors.iter().position(|p| *p == merged) {
            out["members"][name] = variants[i].clone();
        } else {
            require(
                Path::new(name).file_name().and_then(|s| s.to_str()) == Some(name.as_str()),
                "invalid package name",
            )?;
            let file = descriptor(
                format!("{name}-converged-diagnostics-cli-package.json"),
                &merged,
            )?;
            out["members"][name]["manifest"] = file.pin();
            files.push(file);
        }
    }
    files.push(descriptor(DIAGNOSTICS, &out)?);
    Ok(files)
}
fn appended<'a>(base: &Value, branch: &'a Value, expected: Key<'_>) -> Result<&'a Value> {
    let before = components(base)?;
    let after = components(branch)?;
    let (last, first) = after.split_last().ok_or("empty package branch")?;
    require(
        apart_from(base, branch, "components")? && before == first && key(last)? == expected,
        format!("expected one appended {expected:?} component"),
    )?;
    Ok(last)
}
fn check_prefix(base: &Value, prefix: &Value) -> Result {
    let old = components(base)?;
    let new = components(prefix)?;
    require(
        apart_from(base, prefix, "components")? && old.len() == new.len(),
        "terminal prefix package structure changed",
    )?;
    let expected = BTreeSet::from([
        ("cli-surface", "marmish_parser"),
        ("cli-surface", "marmishd_parser"),
    ]);
    let mut changed = BTreeSet::new();
    for (a, b) in old.iter().zip(new) {
        let id = key(a)?;
        require(id == key(b)?, "terminal prefix component order changed")?;
        if a != b {
            require(
                apart_from(a, b, "graph")?
                    && b["graph"]["path"] == "native-cli-prefix-v1/graph.json",
                "unexpected terminal prefix graph or binding",
            )?;
            changed.insert(id);
        }
    }
    require(
        changed == expected,
        "terminal prefix must replace both established CLI graphs",
    )
}
pub(crate) fn canonical(root: &Path) -> Result<Vec<Descriptor>> {
    let root = root.canonicalize()?;
    let public = load(&root.join(PUBLIC))?;
    let mut out = load(&root.join(DIAGNOSTICS))?;
    let prefix = load(&root.join("workspace-v7-terminal-prefix.json"))?;
    let parser = load(&root.join("workspace-v7-converged-release-manifest.json"))?;
    let records = load(&root.join("workspace-v7-observational-records.json"))?;
    check_workspace(&public, &out, Some(&[NATIVE]))?;
    check_workspace(&out, &prefix, Some(&[NATIVE]))?;
    check_workspace(&public, &parser, Some(&[NATIVE]))?;
    check_workspace(&public, &records, Some(&[ENGINE, NATIVE]))?;
    let package =
        |workspace: &Value, name: &str| checked(&root, &workspace["members"][name]["manifest"]);
    let mut native = package(&prefix, NATIVE)?;
    check_prefix(&package(&out, NATIVE)?, &native)?;
    let common = package(&public, NATIVE)?;
    let parsers = package(&parser, NATIVE)?;
    let paths = package(&records, NATIVE)?;
    let parser_row = appended(&common, &parsers, ("line-grammar", "native_manifest"))?;
    let path_row = appended(&common, &paths, ("public-records", "transport"))?;
    native["components"]
        .as_array_mut()
        .ok_or("missing native components")?
        .extend([parser_row.clone(), path_row.clone()]);
    components(&native)?; // Check the additions against the prefix package too.
    appended(
        &package(&public, ENGINE)?,
        &package(&records, ENGINE)?,
        ("public-records", "model"),
    )?;
    let mode = "vault-fixed-value-mode-package.json";
    let bytes = fs::read(root.join(mode))?;
    appended(
        &package(&out, VAULT)?,
        &serde_json::from_slice(&bytes)?,
        ("checked-values", "helper"),
    )?;
    let native = descriptor("marmish-native-canonical-offline-package.json", &native)?;
    out["members"][NATIVE]["manifest"] = native.pin();
    out["members"][ENGINE] = records["members"][ENGINE].clone();
    out["members"][VAULT]["manifest"] = json!({"path":mode,"sha256":bytes_sha(&bytes)});
    Ok(vec![
        native,
        descriptor("workspace-v7-canonical-offline.json", &out)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(module: &str, graph: &str) -> Value {
        json!({"kind":"record","module":module,"graph":graph})
    }
    fn pkg(rows: &[Value]) -> Value {
        json!({"version":"1","components":rows})
    }
    #[test]
    fn merge_components_in_stable_order() -> Result {
        let old = row("z", "base");
        let retained = row("y", "retained");
        let edited = row("z", "changed");
        let a = row("a", "one");
        let b = row("b", "two");
        let base = pkg(&[old, retained.clone()]);
        let left = pkg(&[edited.clone(), retained.clone(), b.clone()]);
        let right = pkg(&[edited.clone(), retained.clone(), a.clone()]);
        let want = pkg(&[edited, retained, a, b]);
        assert_eq!(combine(&base, &[left.clone(), right.clone()])?, want);
        assert_eq!(combine(&base, &[right, left])?, want);
        Ok(())
    }
    #[test]
    fn reject_incompatible_branches() {
        let base = pkg(&[row("a", "base")]);
        let left = pkg(&[row("a", "left")]);
        let right = pkg(&[row("a", "right")]);
        assert!(combine(&base, &[left, right]).is_err());
        for donor in [
            pkg(&[]),
            pkg(&[row("a", "x"), row("a", "y")]),
            json!({"version":"2","components":[row("a","base")]}),
        ] {
            assert!(combine(&base, &[donor]).is_err());
        }
    }
}
