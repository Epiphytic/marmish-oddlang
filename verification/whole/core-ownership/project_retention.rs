use core_qualification::{ownership::*, *};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command};

fn spaced_json(v: &Value) -> Result<String> {
    Ok(match v {
        Value::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| Ok(format!(
                    "{}: {}",
                    pretty(&Value::String(k.clone()))?,
                    spaced_json(v)?
                )))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(spaced_json)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        _ => pretty(v)?,
    })
}
fn main() -> Result {
    let compiler = PathBuf::from(one(std::env::args_os().skip(1))?).canonicalize()?;
    let mut graph = ownership()?;
    remove_native(&mut graph)?;
    let row = one(query(&graph, SEMANTICS)?
        .into_iter()
        .filter(|r| r["type"] == m("retention-policy").into()))?;
    require(
        row["role"] == o("domain").into(),
        "expected a pinned domain",
    )?;
    let relative = named(&row["source"])?
        .as_str()
        .strip_prefix(PIN)
        .ok_or("expected a pinned domain")?;
    let path = portable().join(relative).canonicalize()?;
    require(
        path.starts_with(portable().canonicalize()?),
        "domain escapes portable inputs",
    )?;
    let domain = fs::read(&path)?;
    let digest = format!("{:x}", Sha256::digest(&domain));
    require(row["digest"] == literal(&digest), "wrong domain digest")?;
    let work = tempfile::tempdir()?;
    let input = work.path().join("input");
    let release = work.path().join("output");
    fs::create_dir(&input)?;
    fs::write(input.join("domain.ttl"), domain)?;
    output(
        Command::new(compiler)
            .current_dir(work.path())
            .args(["build", "value-domain", "typescript"])
            .arg(&input)
            .arg(&release),
    )?;
    let lock = release.join("projection.lock");
    require(
        fs::read(&lock)? == fs::read(path.with_file_name("typescript.lock"))?,
        "TypeScript projection lock differs",
    )?;
    let lock = load(&lock)?;
    require(
        lock.get("bindingSha256") == Some(&Value::Null) && lock["artifact"] == "value.mts",
        "unexpected target binding",
    )?;
    for (file, key) in [
        ("value.mts", "artifactSha256"),
        ("obligations.json", "obligationsSha256"),
    ] {
        require(
            lock[key] == sha(&release.join(file))?,
            format!("emitted bytes differ from lock for {file}"),
        )?;
    }
    let node: Value = serde_json::from_str(&output(
        Command::new("node")
            .current_dir(work.path())
            .arg("--experimental-strip-types")
            .arg(whole().join("core-node/check.mjs"))
            .arg(&release),
    )?)?;
    println!(
        "{}",
        spaced_json(
            &json!({"type":named(&row["type"])?.as_str(),"graphSha256":digest,"node":node})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_preserves_sorted_ascii_encoding() -> Result {
        let value = json!({"é": "🌊", "ascii": ["a,b", "c:d"]});
        assert_eq!(
            spaced_json(&value)?,
            r#"{"ascii": ["a,b", "c:d"], "\u00e9": "\ud83c\udf0a"}"#
        );
        Ok(())
    }
}
