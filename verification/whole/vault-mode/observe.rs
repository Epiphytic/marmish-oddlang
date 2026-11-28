//! Observe the enum through a downstream caller and seven rejected trait bounds.
use crate::{Result, require, run, vault_witness::link_command, whole};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
pub fn observe(library: &Path, scratch: &Path, host_deps: &Path) -> Result<Value> {
    let deps = library
        .parent()
        .ok_or("library needs a parent directory")?
        .join("deps");
    let compile = |source: &std::path::Path, output: &std::path::Path| {
        let mut cmd = link_command(source, library, &deps, output, false);
        cmd.arg("-L")
            .arg(format!("dependency={}", host_deps.display()));
        cmd
    };
    let caller = scratch.join("caller");
    run(&mut compile(&whole().join("vault-mode/probe.rs"), &caller))?;
    let values = String::from_utf8(run(&mut Command::new(caller))?)?;
    let source = scratch.join("reject.rs");
    let metadata = scratch.join("reject.rmeta");
    let mut rejected = serde_json::Map::new();
    for bound in [
        "PartialEq",
        "Eq",
        "std::fmt::Debug",
        "std::fmt::Display",
        "Default",
        "std::hash::Hash",
        "Ord",
    ] {
        fs::write(
            &source,
            format!(
                "fn test<T: {bound}>() {{}} fn main() {{test::<marmish_vault::helper::AuthorizationMode>()}}"
            ),
        )?;
        let output = compile(&source, &metadata)
            .args(["--emit=metadata", "--error-format=json"])
            .output()?;
        let mut codes = Vec::new();
        for line in output
            .stderr
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
        {
            let item: Value = serde_json::from_slice(line)?;
            if item["level"] == "error"
                && let Some(code) = item["code"]["code"].as_str()
            {
                codes.push(code.to_owned());
            }
        }
        require(
            !output.status.success() && codes == ["E0277"],
            format!("unexpected {bound} result: {} {codes:?}", output.status),
        )?;
        rejected.insert(bound.to_owned(), json!(codes[0]));
    }
    Ok(json!({"values":values.lines().collect::<Vec<_>>(),"absentTraits":rejected}))
}
