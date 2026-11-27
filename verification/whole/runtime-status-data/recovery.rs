//! Recover the original status record and qualify a source-level field permutation.
use crate::{
    Result,
    assembly::{ORIGINAL, checkout},
    json_bytes, load, portable, require, run, sha,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

const SOURCE: &str = "crates/marmish-native/src/mdk_adapter.rs";
const RECORD: &str = "urn:marmish:native:mdk-adapter:RuntimeStatus";
const FILES: [&str; 3] = ["graph.json", "rust.json", "recovery.lock"];

fn recover(compiler: &Path, source: &Path, selection: &Path, out: &Path) -> Result {
    run(Command::new(compiler)
        .args(["recover", "public-records"])
        .arg(source.with_file_name("lib.rs"))
        .arg(selection)
        .arg(out))?;
    Ok(())
}

fn reordered(mut graph: Value) -> Result<Value> {
    let mut count = 0;
    for module in graph["modules"]
        .as_array_mut()
        .ok_or("missing graph modules")?
    {
        for record in module["records"]
            .as_array_mut()
            .ok_or("missing module records")?
        {
            if record["id"] != RECORD {
                continue;
            }
            let fields = record["fields"]
                .as_array_mut()
                .ok_or("missing status fields")?;
            require(
                fields.get(1).is_some_and(|f| f["wire"] == "conversations")
                    && fields.get(2).is_some_and(|f| f["wire"] == "received"),
                "unexpected original status field order",
            )?;
            fields.swap(1, 2);
            count += 1;
        }
    }
    require(count == 1, "expected exactly one status record")?;
    Ok(graph)
}

pub fn prepare(compiler: &Path, original: &Path, destination: &Path) -> Result {
    checkout(original, ORIGINAL, false)?;
    let dest = std::path::absolute(destination)?;
    require(!dest.try_exists()?, "recovery output already exists")?;
    let stage = tempfile::tempdir_in(dest.parent().ok_or("missing recovery parent")?)?;
    let root = stage.path();
    let domain = portable().join("native-journal-runtime-status-v1");
    let selection = domain.join("selection.json");
    let baseline = root.join("recovered");
    recover(compiler, &original.join(SOURCE), &selection, &baseline)?;
    for name in FILES {
        require(
            fs::read(baseline.join(name))? == fs::read(domain.join(name))?,
            format!("baseline recovery differs at {name}"),
        )?;
    }

    let archive = root.join("original.tar");
    run(Command::new("git")
        .current_dir(original)
        .args(["archive", "--output"])
        .arg(&archive)
        .arg(ORIGINAL))?;
    let tree = root.join("mutated-original");
    fs::create_dir(&tree)?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&tree))?;
    fs::remove_file(archive)?;
    let source = tree.join(SOURCE);
    let text = fs::read_to_string(&source)?;
    let before = "    pub conversations: usize,\n    pub received: usize,\n";
    let after = "    pub received: usize,\n    pub conversations: usize,\n";
    require(
        text.matches(before).count() == 1,
        "ambiguous status field order in original",
    )?;
    let mut spec = load(&selection)?;
    let modules = spec["modules"]
        .as_array_mut()
        .ok_or("missing selection modules")?;
    let mut adapters = modules.iter_mut().filter(|m| m["name"] == "mdk_adapter");
    let module = adapters.next().ok_or("missing adapter")?;
    require(
        adapters.next().is_none() && module["sha256"] == sha(&source)?,
        "ambiguous or stale adapter selection",
    )?;
    fs::write(&source, text.replacen(before, after, 1))?;
    module["sha256"] = sha(&source)?.into();
    let changed_selection = root.join("mutated-selection.json");
    fs::write(&changed_selection, json_bytes(&spec)?)?;
    let changed = root.join("recovered-mutation");
    recover(compiler, &source, &changed_selection, &changed)?;
    require(
        reordered(load(&baseline.join("graph.json"))?)? == load(&changed.join("graph.json"))?,
        "graph changed beyond status field order",
    )?;
    require(
        fs::read(baseline.join("rust.json"))? == fs::read(changed.join("rust.json"))?,
        "Rust binding changed after status field reorder",
    )?;
    renameat_with(CWD, root, CWD, &dest, RenameFlags::NOREPLACE)?;
    Ok(())
}
