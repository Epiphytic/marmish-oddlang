//! Compose the two qualified declaration packages onto the canonical workspace.
use clap::Parser;
use public_data_qualification::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::PathBuf,
};
const ENGINE: &str = "marmish-engine";
const NATIVE: &str = "marmish-native";
const OUTPUT: &str = "workspace-v7-canonical-public-data";

type Key = (String, String, Option<String>);
type Table = toml::value::Table;
type Rows = BTreeMap<Key, Table>;
fn field<'a>(row: &'a Table, key: &str) -> Result<&'a str> {
    Ok(row
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("expected package field {key}"))?)
}
fn lock_rows(raw: &str) -> Result<Rows> {
    let doc: Table = toml::from_str(raw)?;
    require(
        doc.len() == 2 && doc.get("version").and_then(toml::Value::as_integer) == Some(4),
        "unexpected Cargo lock format",
    )?;
    let packages = doc
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing Cargo packages")?;
    let mut rows = Rows::new();
    for p in packages {
        let row = p.as_table().ok_or("invalid Cargo package")?;
        let key = (
            field(row, "name")?.into(),
            field(row, "version")?.into(),
            row.get("source")
                .map(|_| field(row, "source").map(str::to_owned))
                .transpose()?,
        );
        require(
            rows.insert(key, row.clone()).is_none(),
            "duplicate Cargo package",
        )?;
    }
    Ok(rows)
}
fn resolve(rows: &Rows, reference: &str) -> Result<Key> {
    let mut parts = reference.splitn(3, ' ');
    let name = parts.next().unwrap();
    let version = parts.next();
    let source = parts.next();
    let mut matches = rows.keys().filter(|(n, v, s)| {
        n == name
            && version.is_none_or(|value| value == v)
            && source.is_none_or(|value| s.as_ref().is_some_and(|s| format!("({s})") == value))
    });
    let first = matches
        .next()
        .ok_or_else(|| format!("unresolved Cargo reference {reference}"))?;
    require(
        matches.next().is_none(),
        format!("ambiguous Cargo reference {reference}"),
    )?;
    Ok(first.clone())
}
fn engine_closure(raw: &str, rows: &Rows) -> Result<String> {
    let mut pending = vec![resolve(rows, ENGINE)?];
    let mut closure = BTreeMap::new();
    let mut names = HashSet::new();
    while let Some(key) = pending.pop() {
        if closure.contains_key(&key) {
            continue;
        }
        require(
            names.insert(key.0.clone()),
            "ambiguous engine dependency name",
        )?;
        require(
            key.0 == ENGINE || key.2.as_deref().is_some_and(|s| s.starts_with("registry+")),
            "unexpected non-registry dependency in the engine lock",
        )?;
        let row = &rows[&key];
        let mut edges = Vec::new();
        if let Some(deps) = row.get("dependencies") {
            for entry in deps.as_array().ok_or("invalid dependencies")? {
                edges.push(resolve(rows, entry.as_str().ok_or("invalid dependency")?)?);
            }
        }
        pending.extend(edges.iter().cloned());
        closure.insert(key, (row, edges));
    }
    let (header, _) = raw
        .split_once("\n[[package]]")
        .ok_or("missing package blocks")?;
    let mut blocks = vec![header.trim_end().to_owned()];
    for (_, (row, edges)) in closure {
        require(
            row.keys().all(|s| {
                ["name", "version", "source", "checksum", "dependencies"].contains(&s.as_str())
            }),
            "unexpected Cargo package fields",
        )?;
        let mut block = vec!["[[package]]".to_owned()];
        for key in ["name", "version", "source", "checksum"] {
            if row.contains_key(key) {
                block.push(format!(
                    "{key} = {}",
                    serde_json::to_string(field(row, key)?)?
                ));
            }
        }
        if !edges.is_empty() {
            block.push("dependencies = [".into());
            for (name, _, _) in edges {
                block.push(format!(" \"{name}\","));
            }
            block.push("]".into());
        }
        blocks.push(block.join("\n"));
    }
    Ok(blocks.join("\n\n") + "\n")
}
struct Composer {
    base: PathBuf,
    check: bool,
}
impl Composer {
    fn read(&self, name: &str) -> Result<Value> {
        load(&self.base.join(name))
    }
    fn checked(&self, p: &Value) -> Result<PathBuf> {
        let name = text(&p["path"])?;
        require(
            pin(&self.base, name)? == *p,
            format!("input digest changed: {name}"),
        )?;
        Ok(self.base.join(name))
    }
    fn output(&self, name: &str, bytes: &[u8]) -> Result<Value> {
        let path = self.base.join(name);
        if self.check {
            require(
                fs::read(path)? == bytes,
                format!("composition drift: {name}"),
            )?;
        } else {
            fs::write(path, bytes)?;
        }
        pin(&self.base, name)
    }
    fn compose(&self) -> Result {
        let mut base = self.read("workspace-v7-canonical-offline.json")?;
        let choices = self.read("engine-fieldless-choices-package.json")?;
        let engine = load(&self.checked(&base["members"][ENGINE]["manifest"])?)?;
        let choice = list(&choices["components"])?
            .last()
            .ok_or("missing choice")?;
        let mut expected = engine.clone();
        push(&mut expected["components"], choice.clone())?;
        require(
            choices == expected
                && choice["kind"] == "checked-values"
                && choice["module"] == "model"
                && choice["reexports"] == json!(["Stream", "SubmitDisposition"]),
            "unexpected fieldless choice composition",
        )?;
        let reason = self.read("engine-gap-reason-package.json")?;
        let [component] = list(&reason["components"])?.as_slice() else {
            return Err("expected one MissingReason component".into());
        };
        let mut imported = component.clone();
        for (key, file) in [("graph", "graph.json"), ("overlay", "rust.json")] {
            let path = format!("engine-gap-reason-recovered-v1/{file}");
            require(
                load(&self.checked(&imported[key])?)? == self.read(&path)?,
                "recovered MissingReason differs from qualified contract",
            )?;
            imported[key] = pin(&self.base, &path)?;
        }
        let serde = &reason["dependencies"];
        require(
            imported["kind"] == "shared-declarations"
                && imported["module"] == "model"
                && imported["reexports"] == json!(["MissingReason"])
                && serde
                    .as_object()
                    .is_some_and(|m| m.len() == 1 && m.contains_key("serde")),
            "unexpected native MissingReason declaration",
        )?;
        self.checked(&engine["cargoLock"])?;
        self.checked(&reason["cargoLock"])?;
        let raw = fs::read_to_string(self.checked(&base["cargoLock"])?)?;
        let marker = "[[package]]\nname = \"marmish-engine\"\nversion = \"0.1.0\"\ndependencies = [\n \"libc\",\n]";
        require(
            raw.matches(marker).count() == 1,
            "unexpected prior engine dependency block",
        )?;
        let combined = raw.replacen(
            marker,
            &format!("{} \"serde\",\n]", &marker[..marker.len() - 1]),
            1,
        );
        let old = lock_rows(&raw)?;
        let new = lock_rows(&combined)?;
        let key = resolve(&new, ENGINE)?;
        require(
            old.keys().eq(new.keys())
                && old
                    .iter()
                    .filter(|(k, v)| &new[*k] != *v)
                    .map(|(k, _)| k)
                    .eq([&key])
                && new[&key].get("dependencies")
                    == Some(&toml::Value::try_from(["libc", "serde"])?),
            "engine Serde cannot reuse the exact canonical Cargo registry closure",
        )?;
        let mut package = choices;
        package["dependencies"]["serde"] = serde["serde"].clone();
        package["cargoLock"] = self.output(
            &format!("{OUTPUT}/engine-Cargo.lock"),
            engine_closure(&combined, &new)?.as_bytes(),
        )?;
        push(&mut package["components"], imported)?;
        base["members"][ENGINE]["manifest"] = self.output(
            "marmish-engine-canonical-public-data-package.json",
            &json_bytes(&package)?,
        )?;
        let before = load(&self.checked(&base["members"][NATIVE]["manifest"])?)?;
        let status = self.read("workspace-v7-canonical-runtime-status.json")?;
        let profile = &status["members"][NATIVE];
        let native = load(&self.checked(&profile["manifest"])?)?;
        let indices: Vec<_> = list(&before["components"])?
            .iter()
            .enumerate()
            .filter_map(|(i, c)| (c["module"] == "record_views").then_some(i))
            .collect();
        let [index] = indices.as_slice() else {
            return Err("expected one native journal module".into());
        };
        let mut expected = before;
        for key in ["graph", "overlay"] {
            expected["components"][*index][key] = native["components"][*index][key].clone();
        }
        let mut member = base["members"][NATIVE].clone();
        member["manifest"] = profile["manifest"].clone();
        require(
            expected == native && *profile == member,
            "native status candidate changed more than the qualified graph and overlay",
        )?;
        base["members"][NATIVE] = profile.clone();
        base["cargoLock"] = self.output(&format!("{OUTPUT}/Cargo.lock"), combined.as_bytes())?;
        self.output(&format!("{OUTPUT}.json"), &json_bytes(&base)?)?;
        Ok(())
    }
}
fn push(array: &mut Value, value: Value) -> Result {
    array
        .as_array_mut()
        .ok_or("expected components array")?
        .push(value);
    Ok(())
}
#[derive(Parser)]
struct Args {
    #[arg(long)]
    check: bool,
    #[arg(long,default_value_os_t=portable())]
    portable: PathBuf,
}
fn main() -> Result {
    let args = Args::parse();
    Composer {
        base: args.portable,
        check: args.check,
    }
    .compose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_references_require_an_unambiguous_version_and_source() -> Result {
        let rows: Rows = [
            ("serde", "1.0", "registry+one"),
            ("serde", "1.0", "registry+two"),
            ("serde", "2.0", "registry+one"),
        ]
        .into_iter()
        .map(|(name, version, source)| {
            (
                (name.into(), version.into(), Some(source.into())),
                Table::new(),
            )
        })
        .collect();
        assert!(resolve(&rows, "serde").is_err());
        assert!(resolve(&rows, "serde 1.0").is_err());
        assert_eq!(resolve(&rows, "serde 2.0")?.1, "2.0");
        assert_eq!(
            resolve(&rows, "serde 1.0 (registry+two)")?.2.as_deref(),
            Some("registry+two")
        );
        assert!(resolve(&rows, "serde 1.0 (registry+missing)").is_err());
        Ok(())
    }
}
