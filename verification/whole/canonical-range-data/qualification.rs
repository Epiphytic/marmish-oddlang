//! Shared native verification for the authored and recovered MissingRange releases.
use crate::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub const BEFORE: &str = "workspace-v7-canonical-public-data";
pub const AUTHORED: &str = "workspace-v7-canonical-range-data";
pub const RECOVERED: &str = "workspace-v7-canonical-recovered-range";
pub const ENGINE: &str = "marmish-engine";
pub const MODEL: &str = "crates/marmish-engine/src/model.rs";
pub fn host() -> Result<(String, Value)> {
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    require(
        [DARWIN, LINUX].contains(&host)
            && version
                .lines()
                .any(|l| l == format!("commit-hash: {COMPILER}")),
        "requires the pinned native ARM64 toolchain",
    )?;
    Ok((host.into(), json!(version.lines().collect::<Vec<_>>())))
}
pub fn output_path(path: &Path, denied: &[&Path]) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let work = path
        .parent()
        .ok_or("missing output parent")?
        .canonicalize()?
        .join(path.file_name().ok_or("missing output name")?);
    require(
        !denied.iter().any(|p| work.starts_with(p)),
        "choose an independent assembly area",
    )?;
    Ok(work)
}
pub fn tree(root: &Path) -> Result<Value> {
    fn visit(root: &Path, dir: &Path, out: &mut Value) -> Result {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            let key = p
                .strip_prefix(root)?
                .to_str()
                .ok_or("non-UTF-8 tree path")?;
            let f = e.file_type()?;
            if f.is_dir() {
                out[key] = json!({"kind":"directory"});
                visit(root, &p, out)?;
            } else if f.is_file() {
                out[key] = json!({"kind":"file","sha256":sha(&p)?});
            } else {
                return Err(format!("unexpected tree entry: {}", p.display()).into());
            }
        }
        Ok(())
    }
    let mut r = json!({});
    visit(root, root, &mut r)?;
    Ok(r)
}
/// Encode the sorted, ASCII JSON used to sign the historical tree and closure receipts.
pub fn hashed(value: &Value) -> Result<String> {
    fn quote(s: &str) -> Result<String> {
        let mut out = String::new();
        for c in serde_json::to_string(s)?.chars() {
            if c.is_ascii() && c != '\u{7f}' {
                out.push(c);
            } else {
                for u in c.encode_utf16(&mut [0; 2]) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
        Ok(out)
    }
    fn encode(v: &Value) -> Result<String> {
        match v {
            Value::Object(map) => {
                let keys = map.keys().collect::<BTreeSet<_>>();
                let t = keys
                    .into_iter()
                    .map(|k| Ok(format!("{}: {}", quote(k)?, encode(&map[k])?)))
                    .collect::<Result<Vec<_>>>()?;
                Ok(format!("{{{}}}", t.join(", ")))
            }
            Value::Array(a) => Ok(format!(
                "[{}]",
                a.iter().map(encode).collect::<Result<Vec<_>>>()?.join(", ")
            )),
            Value::String(s) => quote(s),
            _ => Ok(v.to_string()),
        }
    }
    Ok(bytes_sha(encode(value)?.as_bytes()))
}
pub fn delta(a: &Value, b: &Value, path: &str) -> Value {
    fn collect(a: &Value, b: &Value, p: &str, out: &mut Value) {
        if a == b {
            return;
        }
        let child = |k: &str| {
            if p.is_empty() {
                k.to_owned()
            } else {
                format!("{p}/{k}")
            }
        };
        match (a, b) {
            (Value::Object(x), Value::Object(y))
                if x.len() == y.len() && x.keys().all(|k| y.contains_key(k)) =>
            {
                for (k, v) in x {
                    collect(v, &y[k], &child(k), out)
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (v, w)) in x.iter().zip(y).enumerate() {
                    collect(v, w, &child(&i.to_string()), out)
                }
            }
            _ => out[p] = json!({"before":a,"after":b}),
        }
    }
    let mut d = json!({});
    collect(a, b, path, &mut d);
    d
}
pub fn keys(v: &Value) -> Result<BTreeSet<String>> {
    Ok(v.as_object()
        .ok_or("expected object")?
        .keys()
        .cloned()
        .collect())
}
pub fn library_closure(raw: &[u8], target: &Path, host: &str) -> Result<Value> {
    let mut paths = BTreeMap::new();
    for row in manifest_native::rows(raw)?
        .into_iter()
        .filter(|row| row["reason"] == "compiler-artifact")
    {
        for name in list(&row["filenames"])? {
            let p = Path::new(text(name)?);
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if ["rlib", "rmeta", if host == LINUX { "so" } else { "dylib" }].contains(&ext) {
                if host == LINUX {
                    require(
                        p.canonicalize()? == p,
                        "symlink in Linux Cargo library closure",
                    )?;
                }
                paths.insert(
                    p.strip_prefix(target)?
                        .to_str()
                        .ok_or("invalid library path")?
                        .to_owned(),
                    sha(p)?,
                );
            }
        }
    }
    require(!paths.is_empty(), "empty library closure")?;
    Ok(json!(paths))
}
pub fn snapshots(base: &Path, product: &Path, prior: &Value, revision: &str) -> Result<Value> {
    let before = text(&prior["workspace"]["candidateWorkspaceLockSha256"])?;
    let hash = sha(&portable().join(AUTHORED).join("workspace.lock"))?;
    let first = product::public_product(base, BEFORE, text(&prior["companion"])?, before)?;
    let last = product::public_product(product, AUTHORED, revision, &hash)?;
    let changes = delta(&first, &last, "");
    let expected = BTreeSet::from([
        "inputSha256".into(),
        format!("members/{ENGINE}/inputSha256"),
        format!("members/{ENGINE}/standalonePackageLockSha256"),
    ]);
    require(
        keys(&changes)? == expected,
        "unexpected workspace member changes",
    )?;
    let orig = base.join(ENGINE);
    let new = product.join(ENGINE);
    let prev = load(&orig.join("package.lock"))?;
    let mut restored = load(&new.join("package.lock"))?;
    let comp = restored["components"]
        .as_array_mut()
        .ok_or("expected engine components")?
        .pop()
        .ok_or("missing appended component")?;
    require(
        load(&portable().join("engine-gap-range-v1/package.lock"))?["components"]
            .as_array()
            .and_then(|s| s.last())
            == Some(&comp),
        "range component differs from standalone",
    )?;
    let a = &prev["generatedFiles"];
    let b = restored["generatedFiles"].clone();
    restored["generatedFiles"] = a.clone();
    restored["manifestSha256"] = prev["manifestSha256"].clone();
    require(
        restored == prev,
        "engine configuration or predecessor components differ",
    )?;
    let idx = list(&prev["components"])?.len();
    let mut added = keys(&b)?;
    for k in keys(a)? {
        require(added.remove(&k), "removed generated engine file")?;
    }
    let changed: BTreeSet<_> = keys(a)?.into_iter().filter(|k| a[k] != b[k]).collect();
    require(
        changed == BTreeSet::from(["src/lib.rs".to_owned(), "src/group_1/mod.rs".to_owned()])
            && added == BTreeSet::from([format!("src/group_1/__projection_{idx}.rs")]),
        "range source footprint differs",
    )?;
    for (f, suffix) in [
        ("src/lib.rs", "pub use model::{MissingRange};\n".into()),
        (
            "src/group_1/mod.rs",
            format!("mod __projection_{idx};\npub use __projection_{idx}::{{MissingRange}};\n"),
        ),
    ] {
        require(
            fs::read(new.join(f))? == [fs::read(orig.join(f))?, suffix.into_bytes()].concat(),
            "unexpected engine export changes",
        )?;
    }
    let mut siblings = keys(&last["members"])?;
    siblings.remove(ENGINE);
    Ok(
        json!({"baselineWorkspaceLockSha256":before,"workspaceLockSha256":hash,"rootCargoLockSha256":last["cargoLockSha256"],"unchangedMemberLocks":siblings,"engineMember":last["members"][ENGINE],"engineChangedFiles":changed,"engineAddedFiles":added,"rangeProjection":comp["projection"]}),
    )
}
pub fn compare(before: &Path, after: &Path) -> Result<Value> {
    let a = tree(before)?;
    let b = tree(after)?;
    require(keys(&a)? == keys(&b)?, "emitted tree path set changed")?;
    let mut same = json!({});
    let mut rust = json!({});
    let mut locks = json!({});
    let mut dirs = 0;
    for name in keys(&a)? {
        let v = &a[&name];
        if v["kind"] == "directory" {
            dirs += 1;
        }
        if v == &b[&name] {
            same[&name] = v.clone();
            if v["kind"] == "file" {
                require(
                    fs::read(before.join(&name))? == fs::read(after.join(&name))?,
                    "inherited bytes changed",
                )?;
                if name.ends_with(".rs") {
                    rust[&name] = v["sha256"].clone();
                }
            }
            continue;
        }
        let allowed: &[&str] = match &*name {
            "workspace.lock" => &[
                "inputSha256",
                "members/marmish-engine/inputSha256",
                "members/marmish-engine/standalonePackageLockSha256",
            ],
            "marmish-engine/package.lock" => &[
                "manifestSha256",
                "components/5/graphSha256",
                "components/5/overlaySha256",
                "components/5/projection/graphSha256",
                "components/5/projection/overlaySha256",
            ],
            _ => return Err(format!("unexpected source change: {name}").into()),
        };
        let d = delta(&load(&before.join(&name))?, &load(&after.join(&name))?, "");
        require(
            keys(&d)? == allowed.iter().map(|s| s.to_string()).collect(),
            "unexpected changed lock fields",
        )?;
        locks[&name] =
            json!({"beforeSha256":v["sha256"],"afterSha256":b[&name]["sha256"],"pins":d});
    }
    require(
        keys(&locks)?.len() == 2,
        "expected both provenance locks to change",
    )?;
    Ok(
        json!({"baselineTreeSha256":hashed(&a)?,"recoveredTreeSha256":hashed(&b)?,"commonTreeSha256":hashed(&same)?,"directories":dirs,"files":keys(&a)?.len()-dirs,"identicalRustFiles":rust,"changedLocks":locks}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn receipt_hash_uses_sorted_ascii_json() -> Result {
        let value = json!({"z":[null,"🌲"],"a":"é\u{7f}"});
        let expected = br#"{"a": "\u00e9\u007f", "z": [null, "\ud83c\udf32"]}"#;
        assert_eq!(hashed(&value)?, bytes_sha(expected));
        Ok(())
    }

    #[test]
    fn equivalence_rejects_an_unexpected_file_and_symlink() -> Result {
        let baseline = tempfile::tempdir()?;
        let changed = tempfile::tempdir()?;
        fs::write(changed.path().join("extra.rs"), b"")?;
        assert!(compare(baseline.path(), changed.path()).is_err());
        symlink(baseline.path(), changed.path().join("outside"))?;
        assert!(tree(changed.path()).is_err());
        Ok(())
    }

    #[test]
    fn lock_delta_retains_exact_field_and_array_coordinates() {
        let old = json!({"components":[{"hash":"before"}],"members":{}});
        let new = json!({"components":[{"hash":"after"}],"members":{}});
        assert_eq!(
            delta(&old, &new, ""),
            json!({"components/0/hash":{"before":"before","after":"after"}})
        );
    }
}
