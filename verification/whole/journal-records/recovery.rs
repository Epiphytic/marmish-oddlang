//! Recover source declarations and verify the two-field permutation independently.
use public_data_qualification::{assembly::ORIGINAL, *};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

pub fn journal(cli: &Path, original: &Path, scratch: &Path) -> Result<Value> {
    let archive = scratch.join("original.tar");
    run(Command::new("git")
        .current_dir(original)
        .args(["archive", "--output"])
        .arg(&archive)
        .arg(ORIGINAL))?;
    let mutant = scratch.join("journal-mutant");
    fs::create_dir(&mutant)?;
    run(Command::new("tar")
        .args(["-xf"])
        .arg(archive)
        .arg("-C")
        .arg(&mutant))?;
    let path = mutant.join("crates/marmish-native/src/journal.rs");
    let raw = fs::read_to_string(&path)?;
    let old = "pub struct ReceivedMessage {\n    pub cursor: u64,\n    pub session: String,\n";
    let new = "pub struct ReceivedMessage {\n    pub session: String,\n    pub cursor: u64,\n";
    require(
        raw.matches(old).count() == 1,
        "source mutation no longer changes exactly the two admitted fields",
    )?;
    fs::write(&path, raw.replacen(old, new, 1))?;
    let bundle = portable().join("native-journal-records-v1");
    let selection = bundle.join("selection.json");
    let baseline = scratch.join("recovered");
    let changed = scratch.join("recovered-reordered");
    let recover = |root: &Path, chosen: &Path, out: &Path| -> Result {
        run(Command::new(cli)
            .args(["recover", "public-records"])
            .arg(root.join("crates/marmish-native/src/lib.rs"))
            .arg(chosen)
            .arg(out))?;
        Ok(())
    };
    recover(original, &selection, &baseline)?;
    for file in ["graph.json", "rust.json", "recovery.lock"] {
        require(
            fs::read(baseline.join(file))? == fs::read(bundle.join(file))?,
            format!("committed record recovery differs: {file}"),
        )?;
    }
    let mut selection = load(&selection)?;
    let matches: Vec<_> = selection["modules"]
        .as_array_mut()
        .ok_or("missing selected modules")?
        .iter_mut()
        .filter(|r| r["name"] == "journal")
        .collect();
    let [entry]: [&mut Value; 1] = matches
        .try_into()
        .map_err(|_| "expected one journal module")?;
    entry["sha256"] = sha(&path)?.into();
    let path = scratch.join("mutated-selection.json");
    fs::write(&path, json_bytes(&selection)?)?;
    recover(&mutant, &path, &changed)?;
    let mut graph = load(&baseline.join("graph.json"))?;
    let mut n = 0;
    for m in graph["modules"]
        .as_array_mut()
        .ok_or("missing recovered modules")?
    {
        for r in m["records"].as_array_mut().ok_or("missing records")? {
            if r["id"] == "urn:marmish:native:journal:ReceivedMessage" {
                let fields = r["fields"]
                    .as_array_mut()
                    .ok_or("missing recovered fields")?;
                require(fields.len() >= 2, "incomplete field permutation")?;
                fields.swap(0, 1);
                n += 1;
            }
        }
    }
    require(
        n == 1
            && graph == load(&changed.join("graph.json"))?
            && fs::read(baseline.join("rust.json"))? == fs::read(changed.join("rust.json"))?,
        "mutation changed the graph beyond the declared field permutation",
    )?;
    let prior = load(&whole().join("journal-records/aarch64-apple-darwin.json"))?;
    require(
        load(&changed.join("recovery.lock"))? == prior["reorderedRecovery"],
        "journal source mutation no longer recovers the qualified graph",
    )?;
    Ok(
        json!({"baselineRecoveryLockSha256":sha(&baseline.join("recovery.lock"))?,"reorderedRecoveryLockSha256":sha(&changed.join("recovery.lock"))?}),
    )
}
