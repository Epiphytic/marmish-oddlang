//! Bind Cargo test results to the exact harness binaries in the named inventory.
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, io, path::Path};
use suite_receipts::Result;

fn digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    io::copy(&mut fs::File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

struct Section {
    key: String,
    doc: bool,
    last: Option<(bool, usize, usize)>,
}

fn summarize(log: &Path, inventory: &Path) -> Result<Value> {
    let baseline = suite_receipts::read(inventory)?;
    let harnesses = baseline["harnesses"]
        .as_object()
        .ok_or("missing harnesses")?;
    let mut keys = BTreeMap::new();
    for (key, row) in harnesses {
        let hash = row["binarySha256"].as_str().ok_or("missing harness hash")?;
        if keys.insert(hash, key).is_some() {
            return Err("ambiguous harness hashes".into());
        }
    }
    let header = Regex::new(r"^\s*Running .+ \((.+)\)$")?;
    let docs = Regex::new(r"^\s*Doc-tests (\S+)\s*$")?;
    let result = Regex::new(
        r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;",
    )?;
    let mut binaries = BTreeMap::new();
    let mut doctests = BTreeMap::new();
    let mut finish = |section: Section| -> Result<()> {
        let (complete, passed, ignored) = section.last.ok_or("unfinished test section")?;
        if !complete {
            return Err(format!("incomplete release run for {}", section.key).into());
        }
        let table = if section.doc {
            &mut doctests
        } else {
            let row = &harnesses[&section.key];
            for (name, observed) in [("tests", passed), ("ignored", ignored)] {
                if row[name].as_array().ok_or("missing named tests")?.len() != observed {
                    return Err(format!("named counts disagree for {}", section.key).into());
                }
            }
            &mut binaries
        };
        if table
            .insert(section.key, json!({"passed":passed,"ignored":ignored}))
            .is_some()
        {
            return Err("duplicate test section".into());
        }
        Ok(())
    };
    let mut active: Option<Section> = None;
    for line in fs::read_to_string(log)?.lines() {
        let new = if let Some(c) = header.captures(line) {
            let path = Path::new(c.get(1).unwrap().as_str());
            if !path.is_absolute() {
                return Err("Cargo target must be absolute".into());
            }
            let key = keys.get(digest(path)?.as_str()).ok_or("unknown harness")?;
            Some((key.to_string(), false))
        } else {
            docs.captures(line).map(|c| (c[1].to_owned(), true))
        };
        if let Some((key, doc)) = new {
            if let Some(prev) = active.replace(Section {
                key,
                doc,
                last: None,
            }) {
                finish(prev)?;
            }
        } else if let (Some(section), Some(c)) = (active.as_mut(), result.captures(line)) {
            let complete = &c[1] == "ok"
                && [3, 5, 6]
                    .into_iter()
                    .all(|i| c[i].parse::<usize>() == Ok(0));
            section.last = Some((complete, c[2].parse()?, c[4].parse()?));
        }
    }
    if let Some(section) = active {
        finish(section)?;
    }
    if binaries.len() != harnesses.len() {
        return Err("missing compiled release harnesses".into());
    }
    let libs: std::collections::BTreeSet<_> = binaries
        .keys()
        .filter(|k| k.contains("/lib/"))
        .filter_map(|k| k.rsplit('/').next())
        .collect();
    if !libs.into_iter().eq(doctests.keys().map(String::as_str)) {
        return Err("unexpected doctest targets".into());
    }
    Ok(
        json!({"upstream":baseline.get("upstream").ok_or("missing upstream")?,
        "namedInventorySha256":digest(inventory)?,"logSha256":digest(log)?,
        "selection":baseline.get("selection").ok_or("missing selection")?,
        "harnesses":binaries,"doctests":doctests}),
    )
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [log, inventory, output] = args.as_slice() else {
        return Err("usage: summarize-cargo LOG NAMED_INVENTORY OUTPUT".into());
    };
    let report = summarize(Path::new(log), Path::new(inventory))?;
    fs::write(
        output,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}
