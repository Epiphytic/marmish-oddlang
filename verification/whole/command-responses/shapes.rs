use super::{ORIGINAL, Result, git_blob, hex, observe, prepare, sha, write_foundation};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path, process::Command};

pub(super) fn compare(
    root: &Path,
    whole: &Path,
    compiler: &Path,
    original: &Path,
) -> Result<Value> {
    let corpus: Vec<(String, String, Option<String>)> =
        serde_json::from_slice(include_bytes!("shape-cases.json"))?;
    let input = root.join("shape-input");
    let mut lines = fs::File::create(&input)?;
    let mut expected = Vec::new();
    for (kind, raw, want) in &corpus {
        writeln!(lines, "{}", json!([kind, hex(raw.as_bytes())]))?;
        expected.push(want.as_ref().map(|v| hex(v.as_bytes())));
    }
    drop(lines);
    let model = whole.join("../../oddlang/portable/command-response-shapes-v1");
    let projection = root.join("projection");
    if !Command::new(compiler)
        .args(["build", "wire-schema", "rust"])
        .arg(model.join("graph.json"))
        .arg(model.join("rust.json"))
        .arg(&projection)
        .status()?
        .success()
    {
        return Err("shape projection failed".into());
    }
    let source = fs::read(projection.join("wire.rs"))?;
    let mut observations = Vec::new();
    for reference in [false, true] {
        let dir = root.join(if reference { "reference" } else { "candidate" });
        fs::create_dir(&dir)?;
        prepare(&dir, &whole.join("command-requests"))?;
        fs::copy(
            whole.join("command-responses/adapter.rs"),
            dir.join("peer.rs"),
        )?;
        let mut flags = String::from("-Adead_code --check-cfg=cfg(reference)");
        if reference {
            fs::write(dir.join("codec.rs"), git_blob(original, "command_wire.rs")?)?;
            write_foundation(&dir, original)?;
            flags.push_str(" --cfg reference");
        } else {
            fs::write(dir.join("codec.rs"), &source)?;
        }
        observations.push(observe(&dir, &flags, &input)?);
    }
    if observations[0] != observations[1] {
        return Err("reference and projected response shapes differ".into());
    }
    let actual: Vec<Option<String>> = serde_json::Deserializer::from_slice(&observations[0])
        .into_iter()
        .collect::<std::result::Result<_, _>>()?;
    if actual != expected {
        return Err("shape outputs differ from frozen expectations".into());
    }
    Ok(
        json!({"referenceRevision":ORIGINAL,"cases":corpus.len(),"accepted":actual.iter().flatten().count(),"observationsSha256":sha(&observations[0]),"generatedSourceSha256":sha(&source)}),
    )
}
