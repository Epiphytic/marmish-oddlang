//! Compare public imports inherited by the diagnostics and canonical macOS workspaces.
use public_data_qualification::{
    Result, artifacts::selected_build, assembly::DARWIN, imports, list, load, require, text, whole,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn verdicts(library: &Value, count: usize) -> Result<BTreeMap<&str, bool>> {
    let mut found = BTreeMap::new();
    for row in list(&library["imports"])? {
        require(
            found
                .insert(
                    text(&row["spelling"])?,
                    row["compiles"]
                        .as_bool()
                        .ok_or("missing public import outcome")?,
                )
                .is_none(),
            "duplicate public import",
        )?;
    }
    require(
        found.len() == count,
        "unexpected public import inventory size",
    )?;
    Ok(found)
}
fn comparison(previous: &Value, current: &Value) -> Result<(Vec<(String, String)>, usize)> {
    let mut additions = Vec::new();
    let mut available = 0;
    for (name, count) in imports::COUNT {
        let old = verdicts(&previous[name], count)?;
        let new = verdicts(&current[name], count)?;
        require(old.keys().eq(new.keys()), "public import inventory changed")?;
        let passed = new.values().filter(|ok| **ok).count();
        require(
            current[name]["importable"] == passed,
            "inconsistent public import count",
        )?;
        available += passed;
        for (key, success) in new {
            require(
                !old[key] || success,
                format!("lost inherited public import {name}::{key}"),
            )?;
            if success && !old[key] {
                additions.push((name.to_string(), key.to_string()));
            }
        }
    }
    Ok((additions, available))
}
pub fn observe(
    profile: &str,
    product: &Path,
    target: &Path,
    original: &Path,
    raw: &str,
) -> Result<Value> {
    let previous = match profile {
        "diagnostics" => {
            load(&whole().join("converged-public-api/darwin-imports.json"))?["imports"].take()
        }
        "canonical" => load(&whole().join("converged-diagnostics-cli/aarch64-apple-darwin.json"))?
            ["publicApi"]["libraries"]
            .take(),
        _ => return Err("unknown public import profile".into()),
    };
    let names = imports::COUNT.map(|(name, _)| format!("marmish-{name}"));
    let (roles, links) = selected_build(
        raw.as_bytes(),
        &product.canonicalize()?,
        &target.canonicalize()?,
        &names.each_ref().map(String::as_str),
    )?;
    let libraries = imports::library_set(DARWIN, original, &roles)?;
    let (added, available) = comparison(&previous, &libraries)?;
    let count: usize = imports::COUNT.iter().map(|(_, n)| n).sum();
    if profile == "canonical" {
        return Ok(
            json!({"darwinPublicImports":{"attempted":count,"importable":available,"additional":added,"libraries":libraries}}),
        );
    }
    require(
        available == 71 && added == [("native".into(), "diagnostics".into())],
        "unexpected diagnostics import delta",
    )?;
    Ok(
        json!({"roles":roles,"links":links,"publicApi":{"spellingCount":count,"importable":available,"delta":added,"libraries":libraries}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_lost_spelling_and_duplicate_verdicts() -> Result {
        let src =
            load(&whole().join("converged-public-api/darwin-imports.json"))?["imports"].take();
        let mut next = src.clone();
        next["core"]["imports"][0]["spelling"] = "changed".into();
        assert!(comparison(&src, &next).is_err());
        next = src.clone();
        next["core"]["imports"][0] = next["core"]["imports"][1].clone();
        assert!(comparison(&src, &next).is_err());
        let mut lost = src.clone();
        let key = imports::COUNT
            .iter()
            .map(|x| x.0)
            .find(|k| src[*k]["imports"][0]["compiles"] == true)
            .unwrap();
        lost[key]["imports"][0]["compiles"] = false.into();
        lost[key]["importable"] = (src[key]["importable"].as_u64().unwrap() - 1).into();
        assert!(comparison(&src, &lost).is_err());
        Ok(())
    }
}
