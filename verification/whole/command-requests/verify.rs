use super::{ORIGINAL, Result, compare_codecs, hex, sha, workspace_evidence};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path, process::Command};

pub(super) fn compare(
    root: &Path,
    whole: &Path,
    compiler: &Path,
    original: &Path,
    package: Option<&Path>,
    workspace_lock: Option<&Path>,
) -> Result<Value> {
    let portable = whole.join("../../oddlang/portable");
    let model = portable.join("command-requests-v1");
    let dir = root.join("projection");
    let status = Command::new(compiler)
        .args(["build", "wire-protocol", "rust"])
        .args(["shape.json", "shape.rust.json", "graph.json", "rust.json"].map(|p| model.join(p)))
        .arg(&dir)
        .status()?;
    if !status.success() {
        return Err("request projection failed".into());
    }
    let source = fs::read(dir.join("wire.rs"))?;
    let historical = portable.join("workspace-basic-commands/workspace.lock");
    let workspace = package
        .map(|path| {
            workspace_evidence(
                path,
                workspace_lock.unwrap_or(&historical),
                "command_requests",
                &source,
            )
        })
        .transpose()?;
    let cases: Vec<(String, String, String, bool)> =
        serde_json::from_slice(include_bytes!("cases.json"))?;
    let input = root.join("request-input");
    let mut lines = fs::File::create(&input)?;
    let mut expected = Vec::new();
    for (operation, _, raw, accepted) in &cases {
        let wire = hex(raw.as_bytes());
        writeln!(lines, "{}", json!([operation, wire]))?;
        expected.push(accepted.then_some(wire));
    }
    drop(lines);
    let adapters = whole.join("command-requests");
    let observed = compare_codecs(
        root,
        whole,
        original,
        &source,
        &input,
        &adapters.join("generated.rs"),
        &adapters.join("original.rs"),
    )?;
    let actual: Vec<Option<String>> = serde_json::Deserializer::from_slice(&observed)
        .into_iter()
        .collect::<std::result::Result<_, _>>()?;
    if actual.len() != expected.len() {
        return Err("incomplete request observations".into());
    }
    for ((got, want), (op, name, _, _)) in actual.iter().zip(&expected).zip(&cases) {
        if got != want {
            return Err(format!("incorrect request observation for {op}/{name}").into());
        }
    }
    let mut receipt = json!({"original":ORIGINAL,"cases":cases.len(),"accepted":expected.iter().flatten().count(),"observationsSha256":sha(&observed),"wireSha256":sha(&source),"compilerSha256":sha(&fs::read(compiler)?)});
    if let Some(workspace) = workspace {
        receipt["workspace"] = workspace;
    }
    Ok(receipt)
}
