//! Compose the inert directory catalog from the pinned file policy and manifest grammar.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::bytes_sha,
    cli_projection::{REVISION, input_bytes},
    json_bytes, list, load, pin, portable, require, run, text,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const MEMBER: &str = "marmish-native";
const BASE: &str = "workspace-v7-local-file-integrity.json";
const SCOPE: &str = "local-file-catalog-v1";
const WORKSPACE: &str = "workspace-v7-local-file-catalog";
const PACKAGE: &str = "native-local-file-catalog-package.json";

#[derive(Parser)]
struct Args {
    original: PathBuf,
    #[arg(long)]
    check: bool,
    #[arg(long, default_value_os_t=portable())]
    portable: PathBuf,
}

fn blocks(raw: &str) -> Result<(&str, Vec<(String, &str)>)> {
    let mut chunks = raw.split("[[package]]\n");
    let header = chunks.next().ok_or("empty Cargo lock")?;
    let mut rows = Vec::new();
    for chunk in chunks {
        let row: toml::Table = toml::from_str(chunk)?;
        let name = row
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or("expected package name")?;
        rows.push((name.to_owned(), chunk));
    }
    Ok((header, rows))
}

fn fixture_lock(previous: &[u8], upstream: &str) -> Result<Vec<u8>> {
    let (header, rows) = blocks(std::str::from_utf8(previous)?)?;
    let fixtures = ["fastrand", "tempfile"];
    let mut all = Vec::new();
    let mut native_count = 0;
    for (name, raw) in rows {
        require(
            !fixtures.contains(&name.as_str()),
            "fixture provider already in predecessor",
        )?;
        let mut block = raw.to_owned();
        if name == MEMBER {
            native_count += 1;
            let anchor = " \"tokio\",\n";
            require(
                raw.matches(anchor).count() == 1,
                "expected one native Tokio dependency",
            )?;
            block = raw.replace(anchor, " \"tempfile\",\n \"tokio\",\n");
            let mut expected: toml::Table = toml::from_str(raw)?;
            let deps = expected
                .get_mut("dependencies")
                .and_then(toml::Value::as_array_mut)
                .ok_or("missing native dependencies")?;
            deps.push(toml::Value::String("tempfile".into()));
            deps.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            require(
                expected == toml::from_str::<toml::Table>(&block)?,
                "fixture changed another dependency",
            )?;
        }
        all.push((name, block));
    }
    require(native_count == 1, "expected exactly one native package")?;
    let (_, upstream_rows) = blocks(upstream)?;
    for name in fixtures {
        let mut candidates = upstream_rows.iter().filter(|(n, _)| n == name);
        let (_, block) = candidates.next().ok_or("missing original test provider")?;
        require(
            candidates.next().is_none(),
            "ambiguous original test provider",
        )?;
        all.push((name.to_owned(), (*block).to_owned()));
    }
    all.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lock = header.to_owned();
    for (_, row) in all {
        lock.push_str("[[package]]\n");
        lock.push_str(&row);
    }
    toml::from_str::<toml::Table>(&lock)?;
    Ok(lock.into_bytes())
}

fn checked_bytes(base: &Path, descriptor: &Value) -> Result<Vec<u8>> {
    let rel = Path::new(text(&descriptor["path"])?);
    require(
        descriptor
            .as_object()
            .is_some_and(|fields| fields.len() == 2)
            && rel
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "invalid relative input pin",
    )?;
    Ok(input_bytes(base, descriptor)?.1)
}

fn pinned_json(base: &Path, descriptor: &Value) -> Result<Value> {
    Ok(serde_json::from_slice(&checked_bytes(base, descriptor)?)?)
}
fn component<'a>(package: &'a Value, key: &str, value: &str) -> Result<&'a Value> {
    let mut items = list(&package["components"])?
        .iter()
        .filter(|c| c[key] == value);
    let item = items.next().ok_or("missing required component")?;
    require(items.next().is_none(), "ambiguous catalog component")?;
    Ok(item)
}
fn graph(grammar: &Value, files: &Value) -> Result<Value> {
    let mut literals = BTreeMap::new();
    for line in list(&grammar["rows"])? {
        for row in list(line)? {
            if row["kind"] == "literal" && row.get("capture").is_some() {
                literals.insert(text(&row["capture"])?, text(&row["text"])?);
            }
        }
    }
    let mut names = Vec::new();
    for item in list(&grammar["recordResult"]["entry"]["fields"]["files"]["items"])? {
        let id = text(&item["fields"]["name"]["id"])?;
        names.push(*literals.get(id).ok_or("unbound asset name")?);
    }
    let anchor = names.first().ok_or("empty asset list")?;
    let [version] = list(&grammar["textGrammars"])?.as_slice() else {
        return Err("expected one version grammar".into());
    };
    let prefixes = list(&version["prefixes"])?
        .iter()
        .map(|s| Ok(format!("{anchor}-{}", text(s)?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"format":"oddlang-local-file-catalog-v1","id":"urn:marmish:native:read-only-file-catalog:v1",
        "manifestGrammar":text(&grammar["id"])?,"filePolicy":text(&files["profile"]["id"])?,"fixedCatalog":"manifest",
        "prefixes":prefixes,"candidateNameEncoding":"utf8","attributeChecks":"fixed-owner-and-mode","suffix":".manifest",
        "namingAnchor":anchor,"assetNames":names,"maximumCatalogBytes":grammar["maxBytes"].as_u64().ok_or("invalid catalog byte bound")?,"digest":"sha256",
        "guards":["unique-name","bounded-read","catalog-digest","parse","canonical-name","ordered-assets"],
        "error":{"id":"urn:marmish:native:error:v1:variant:Invalid","ambiguous":"install_manifest_path","digest":"install_digest"}}),
    )
}
fn encoded(mut value: Value) -> Result<Vec<u8>> {
    value.sort_all_objects();
    json_bytes(&value)
}
fn main() -> Result<()> {
    let args = Args::parse();
    let base = args.portable.canonicalize()?;
    let upstream = String::from_utf8(run(Command::new("git")
        .current_dir(args.original)
        .args(["show", &format!("{REVISION}:Cargo.lock")]))?)?;
    let mut workspace = load(&base.join(BASE))?;
    let mut native = pinned_json(&base, &workspace["members"][MEMBER]["manifest"])?;
    let grammar = pinned_json(
        &base,
        &component(&native, "module", "native_manifest")?["graph"],
    )?;
    let files = pinned_json(
        &base,
        &component(&native, "kind", "read-only-file")?["graph"],
    )?;
    let mut outputs = Vec::new();
    let mut add = |path: String, bytes: Vec<u8>| {
        let descriptor = json!({"path":path,"sha256":bytes_sha(&bytes)});
        outputs.push((path, bytes));
        descriptor
    };
    let spec = add(
        format!("{SCOPE}/graph.json"),
        encoded(graph(&grammar, &files)?)?,
    );
    let overlay = pin(&base, &format!("{SCOPE}/rust.json"))?;
    native["components"].as_array_mut().ok_or("missing components")?.push(json!({"kind":"local-file-catalog","module":"native_manifest",
        "reexports":[],"crateReexports":["AssetNaming","NAMES","verify_directory"],"graph":spec,"overlay":overlay}));
    for (doc, scope) in [(&mut native, SCOPE), (&mut workspace, WORKSPACE)] {
        let old = checked_bytes(&base, &doc["cargoLock"])?;
        doc["cargoLock"] = add(
            format!("{scope}/Cargo.lock"),
            fixture_lock(&old, &upstream)?,
        );
    }
    native["devDependencies"] = json!({"tempfile":{"version":"3.27.0"}});
    workspace["members"][MEMBER]["manifest"] = add(PACKAGE.into(), encoded(native)?);
    add(format!("{WORKSPACE}.json"), encoded(workspace)?);
    for (name, bytes) in outputs {
        let dest = base.join(&name);
        if args.check {
            require(
                fs::read(dest)? == bytes,
                format!("changed generated input {name}"),
            )?;
        } else {
            let parent = dest.parent().ok_or("missing output parent")?;
            fs::create_dir_all(parent)?;
            use std::os::unix::fs::PermissionsExt;
            let permissions = match fs::metadata(&dest) {
                Ok(meta) => meta.permissions(),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    fs::Permissions::from_mode(0o666)
                }
                Err(err) => return Err(err.into()),
            };
            let mut file = tempfile::Builder::new()
                .permissions(permissions)
                .tempfile_in(parent)?;
            file.write_all(&bytes)?;
            file.persist(dest)?;
        }
    }
    Ok(())
}
