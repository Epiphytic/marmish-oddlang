use super::{ORIGINAL, Result, compare_codecs, hex, sha};
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
    let adapter = whole.join("command-responses/adapter.rs");
    let output = compare_codecs(root, whole, original, &source, &input, &adapter, &adapter)?;
    let actual: Vec<Option<String>> = serde_json::Deserializer::from_slice(&output)
        .into_iter()
        .collect::<std::result::Result<_, _>>()?;
    if actual != expected {
        return Err("shape outputs differ from frozen expectations".into());
    }
    Ok(
        json!({"referenceRevision":ORIGINAL,"cases":corpus.len(),"accepted":actual.iter().flatten().count(),"observationsSha256":sha(&output),"generatedSourceSha256":sha(&source)}),
    )
}
