use clap::Parser;
use copy_checkpoint_replay::{compare, consumer, observe};
use copy_manifest_replay::{Cases, Release, launch_data, launch_hash};
use public_data_qualification::{Result, load, portable, require, whole};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}

fn remove(obj: &mut Value, key: &str) -> Result<Value> {
    obj.as_object_mut()
        .and_then(|m| m.shift_remove(key))
        .ok_or_else(|| format!("missing {key}").into())
}

fn check_base() -> Result {
    let base = portable().join("copy-launch-v1");
    let graphs = portable().join("copy-launch-hash-v1");
    let mut graph = load(&graphs.join("launch.json"))?;
    let tags = remove(&mut graph, "hashTags")?;
    remove(&mut graph, "hashViews")?;
    let codec = "urn:marmish:stream:codec";
    let types = graph["types"].as_array_mut().ok_or("missing types")?;
    let i = types
        .iter()
        .position(|t| t == &json!({"kind":"external","id":codec}))
        .ok_or("missing codec")?;
    types.remove(i);
    require(
        graph == load(&base.join("launch.json"))?,
        "base Launch contract differs",
    )?;
    for e in load(&base.join("enums.json"))?["stringEnums"]
        .as_array()
        .ok_or("missing enums")?
    {
        if let Some(t) = tags.get(e["id"].as_str().ok_or("missing enum ID")?) {
            require(t == &e["numbers"]["tags"], "enum tags differ")?;
        }
    }
    let mut hints = load(&graphs.join("launch-rust.json"))?;
    remove(&mut hints, "hashViews")?;
    remove(&mut hints, "hashVariants")?;
    remove(&mut hints["externals"], codec)?;
    require(
        hints == load(&base.join("launch-rust.json"))?,
        "base Launch overlay differs",
    )
}

fn data_checks(original: &Path, generated: &Path, work: &Path) -> Result<Value> {
    let caller = whole().join("copy-launch/caller.rs");
    let (creates, expected) = launch_data::constructs();
    let decodes = launch_data::decodes();
    let old = consumer(
        &original.join("crates/marmish-stream"),
        &work.join("original-data"),
        &original.join("Cargo.lock"),
        &caller,
    )?;
    let new = consumer(
        generated,
        &work.join("generated-data"),
        &generated.join("Cargo.lock"),
        &caller,
    )?;
    for (role, exe) in [("original", &old), ("generated", &new)] {
        compare(role, exe, &creates, &expected)?;
    }
    let output = observe(&old, &decodes)?;
    let baseline = std::str::from_utf8(&output)?
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<Vec<Value>>>()?;
    compare("generated decode", &new, &decodes, &baseline)?;
    Ok(json!({"constructCases":creates.len(),"exactDecodeCases":decodes.len()}))
}

fn mutate(graph: &mut Value) -> Result {
    for view in graph["hashViews"].as_array_mut().ok_or("missing views")? {
        let first = if view["output"]["kind"] == "bytes" {
            2
        } else {
            0
        };
        let fields = view["fields"].as_array_mut().ok_or("missing fields")?;
        require(fields.len() > first + 1, "missing slots for reorder")?;
        fields.swap(first, first + 1);
    }
    Ok(())
}

fn main() -> Result {
    let args = Args::parse();
    check_base()?;
    let rows = launch_hash::rows();
    let expected: Vec<_> = rows.iter().map(|r| r["expected"].clone()).collect();
    let options = expected.get(..64).ok_or("missing flag combinations")?;
    let distinct = |key: &str| {
        options
            .iter()
            .map(|r| r[key].to_string())
            .collect::<HashSet<_>>()
            .len()
    };
    require(
        distinct("namespace") == 2 && distinct("digest") == 64,
        "invalid option coverage",
    )?;
    let first = &rows[0];
    require(
        ["digest", "namespace"]
            .iter()
            .all(|k| first["swapped"][k] != first["expected"][k]),
        "mutation lacks a distinct expected result",
    )?;
    let cases = Cases {
        metadata: json!({"cases":rows.len(),"invalidDigestCases":expected.iter().filter(|v|v["digest"].get("error").is_some()).count()}),
        requests: rows.iter().map(|r| r["request"].clone()).collect(),
        expected,
        mutant: vec![(first["request"].clone(), first["swapped"].clone())],
        negative: "reordering fixed-width field slots changes digest and namespace",
    };
    Release {
        lane: "copy-launch-hash",
        package: "launch-hash",
        framework: "217da3c13d504b622e0be4efce2eaf8bb51b05f0",
        digest_key: "expectationsSha256",
    }
    .verify(
        &args.original,
        &args.framework,
        &args.receipt,
        cases,
        mutate,
        data_checks,
    )
}
