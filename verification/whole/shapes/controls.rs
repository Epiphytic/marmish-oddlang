//! Change real archive copies and check that the verifier identifies each broken obligation.
use super::*;
use std::io::Write;

fn store(path: &Path, mut doc: Value) -> Result<()> {
    doc.sort_all_objects();
    fs::write(path, public_data_qualification::json_bytes(&doc)?)?;
    Ok(())
}
fn lock_path(root: &Path, cell: &Value) -> Result<PathBuf> {
    Ok(root.join("locks").join(format!("{}.json", key(cell)?)))
}
fn graph_path(root: &Path, cell: &Value) -> Result<PathBuf> {
    Ok(root
        .join("graphs")
        .join(format!("{}.json", text(&cell["graphSha256"])?)))
}
fn repin_lock(root: &Path, cell: &mut Value, mut lock: Value) -> Result<()> {
    let path = lock_path(root, cell)?;
    store(&path, lock.clone())?;
    cell["lockSha256"] = sha(&path)?.into();
    lock["pin"]
        .as_object_mut()
        .ok_or("missing profile pin")?
        .remove("cargoConfigSha256");
    lock.sort_all_objects();
    cell["semanticLockSha256"] = bytes_sha(&ascii_json(&serde_json::to_string(&lock)?)).into();
    Ok(())
}
fn repin_graph(root: &Path, cells: &mut [Value], path: &Path, graph: Value) -> Result<()> {
    let old = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("invalid graph path")?;
    store(path, graph)?;
    let digest = sha(path)?;
    fs::rename(path, path.with_file_name(format!("{digest}.json")))?;
    for cell in cells.iter_mut().filter(|c| c["graphSha256"] == old) {
        cell["graphSha256"] = digest.clone().into();
        let mut lock = load(&lock_path(root, cell)?)?;
        lock["shapesSha256"] = digest.clone().into();
        repin_lock(root, cell, lock)?;
    }
    Ok(())
}
fn first_node<'a>(v: &'a mut Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a mut Value> {
    if predicate(v) {
        return Some(v);
    }
    match v {
        Value::Object(m) => m.values_mut().find_map(|n| first_node(n, predicate)),
        Value::Array(a) => a.iter_mut().find_map(|n| first_node(n, predicate)),
        _ => None,
    }
}
const CASES: &[(&str, &str)] = &[
    ("target", "target inventory"),
    ("cell", "missing or extra profile cells"),
    ("graph-missing", "No such file"),
    ("graph-hash", "graph digest mismatch"),
    ("lock-hash", "lock digest mismatch"),
    ("features", "profile pin mismatch"),
    ("field-order", "invalid field order"),
    ("mac-source-null", "missing or invalid source revision"),
    ("mac-source-absent", "missing or invalid source revision"),
    ("linux-source-null", "missing or invalid source revision"),
    ("linux-source-absent", "missing or invalid source revision"),
    ("missing-gap", "missing required gaps"),
    ("unmatched-gap", "unmatched unresolved gap"),
    ("unknown-gap", "unknown gap reason"),
    ("local-reference", "unknown local declaration"),
    ("integer-width", "invalid unsigned-integer width"),
    ("root-file", "invalid archive root inventory"),
];
fn change(root: &Path, case: &str) -> Result<()> {
    let release = root.join("release.json");
    let mut manifest = load(&release)?;
    let cells = manifest["cells"].as_array_mut().ok_or("missing cells")?;
    let core = cells
        .iter()
        .position(|r| r["target"] == "marmish_core" && r["status"] == "candidate")
        .ok_or("missing core profile")?;
    let stream = cells
        .iter()
        .position(|r| r["target"] == "marmish_stream" && r["triple"] == TRIPLES[0])
        .ok_or("missing stream profile")?;
    let path = graph_path(root, &cells[core])?;
    let lock = lock_path(root, &cells[core])?;
    match case {
        "target" => {
            manifest["targets"]
                .as_array_mut()
                .ok_or("missing targets")?
                .pop();
        }
        "cell" => {
            cells.pop();
        }
        "graph-missing" => fs::remove_file(path)?,
        "graph-hash" | "lock-hash" => fs::OpenOptions::new()
            .append(true)
            .open(if case == "graph-hash" { path } else { lock })?
            .write_all(b" ")?,
        "root-file" => fs::write(root.join("extra.json"), b"{}")?,
        "features" => {
            let mut profiles = cells
                .iter_mut()
                .filter(|r| r["target"] == "marmish_engine" && r["triple"] == TRIPLES[0]);
            let base = profiles.next().ok_or("missing baseline engine")?;
            let fault = profiles.next().ok_or("missing fault engine")?;
            require(
                base["graphSha256"] == fault["graphSha256"],
                "expected shared graph",
            )?;
            repin_lock(root, base, load(&lock_path(root, fault)?)?)?;
        }
        x if x.contains("-source-") => {
            let cell = if x.starts_with("mac-") {
                &mut cells[core]
            } else {
                cells
                    .iter_mut()
                    .find(|c| {
                        c["status"] == "candidate"
                            && c["triple"].as_str().is_some_and(|t| t.contains("linux"))
                    })
                    .ok_or("missing Linux candidate")?
            };
            let mut lock = load(&lock_path(root, cell)?)?;
            if x.ends_with("-null") {
                lock["pin"]["sourceRevision"] = Value::Null;
            } else {
                lock["pin"]
                    .as_object_mut()
                    .ok_or("missing pin")?
                    .remove("sourceRevision");
            }
            repin_lock(root, cell, lock)?;
        }
        _ => {
            let path = if matches!(case, "missing-gap" | "local-reference" | "integer-width") {
                graph_path(root, &cells[stream])?
            } else {
                path
            };
            let mut graph = load(&path)?;
            match case {
                "field-order" => {
                    let row = first_node(&mut graph, &|n| {
                        n["fields"].as_array().is_some_and(|f| f.len() >= 2)
                    })
                    .ok_or("missing fields")?;
                    row["fields"].as_array_mut().unwrap().swap(0, 1);
                }
                "local-reference" => {
                    first_node(&mut graph, &|n| {
                        n["kind"] == "named"
                            && n["id"].as_str().is_some_and(|s| s.starts_with("local::"))
                    })
                    .ok_or("missing local reference")?["id"] =
                        "local::marmish_stream::missing::{type}".into();
                }
                "integer-width" => {
                    first_node(&mut graph, &|n| n["kind"] == "unsigned-integer")
                        .ok_or("missing integer")?["bits"] = 7.into();
                }
                _ => {
                    let owner = graph["declarations"][0]["id"].clone();
                    let gaps = graph["gaps"].as_array_mut().ok_or("missing graph gaps")?;
                    match case {
                        "missing-gap" => gaps.retain(|g| g["reason"] != "array-length"),
                        "unmatched-gap" => {
                            gaps.push(json!({"owner":owner,"reason":"array-length"}))
                        }
                        "unknown-gap" => gaps[0]["reason"] = "fabricated-reason".into(),
                        _ => return Err("unknown control".into()),
                    }
                    gaps.sort_by(|a, b| {
                        (a["owner"].as_str(), a["reason"].as_str())
                            .cmp(&(b["owner"].as_str(), b["reason"].as_str()))
                    });
                }
            }
            repin_graph(root, cells, &path, graph)?;
        }
    }
    store(&release, manifest)
}
pub fn run_controls(original: &Path, framework: &Path, archive: &Path) -> Result<()> {
    verify(original, framework, archive)?;
    for &(case, expected) in CASES {
        let work = tempfile::tempdir()?;
        let root = work.path().join("archive");
        run(Command::new("cp").arg("-R").arg(archive).arg(&root))?;
        change(&root, case)?;
        let error = verify(original, framework, &root)
            .err()
            .ok_or_else(|| format!("control {case} passed"))?;
        require(
            error.to_string().contains(expected),
            format!("control {case}: {error}"),
        )?;
        println!("{case}");
    }
    Ok(())
}
