//! Original-source recovery and external native probes shared with the integrated workspace gate.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub const PROFILES: [(&str, &str, &str); 2] = [
    ("finite-error-domain", "error", "AdapterError"),
    ("public-root-records", "record", "VersionObservation"),
];
pub fn here() -> PathBuf {
    whole().join("mdk-public-data")
}
pub fn recover(cli: &Path, source: &Path, work: &Path, inputs: &Path) -> Result<Value> {
    fs::create_dir_all(inputs)?;
    let mut facts = json!({});
    for (kind, role, _) in PROFILES {
        let name = format!("{role}-selection.json");
        let mut selection = load(&here().join(&name))?;
        selection["sourceSha256"] = sha(source)?.into();
        let chosen = work.join(name);
        fs::write(&chosen, json_bytes(&selection)?)?;
        let extract = work.join(format!("extracted-{role}"));
        run(Command::new(cli)
            .args(["recover", kind])
            .arg(source)
            .arg(chosen)
            .arg(&extract))?;
        for (file, suffix) in [("graph.json", ".json"), ("rust.json", "-rust.json")] {
            fs::copy(extract.join(file), inputs.join(format!("{role}{suffix}")))?;
        }
        let witness = if role == "error" {
            "observation.json"
        } else {
            "recovery.lock"
        };
        facts[role] = load(&extract.join(witness))?;
    }
    Ok(facts)
}
pub fn observe(
    library: &Path,
    deps: &Path,
    serde: &Path,
    serde_deps: &Path,
    work: &Path,
) -> Result<(Vec<u8>, Value)> {
    fs::create_dir(work)?;
    let rust = || {
        let mut cmd = Command::new("rustc");
        cmd.args(["+1.97.1", "--edition=2021"])
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-L")
            .arg(format!("dependency={}", serde_deps.display()))
            .arg("--extern")
            .arg(format!("marmish_mdk={}", library.display()))
            .arg("--extern")
            .arg(format!("serde={}", serde.display()));
        cmd
    };
    let binary = work.join("probe");
    run(rust().arg(here().join("probe.rs")).arg("-o").arg(&binary))?;
    let transcript = run(&mut Command::new(&binary))?;
    let mut cases = vec![(
        "rootResult".to_owned(),
        "E0432",
        "use marmish_mdk::Result;\nfn main() {}".to_owned(),
    )];
    for name in ["VersionObservation", "AdapterError"] {
        for (label, bound) in [
            ("Clone", "Clone"),
            ("Copy", "Copy"),
            ("Eq", "Eq"),
            ("PartialEq", "PartialEq"),
            ("Default", "Default"),
            ("Serialize", "serde::Serialize"),
            ("Deserialize", "for<'de> serde::Deserialize<'de>"),
            ("Error", "std::error::Error"),
        ] {
            if label == "Error" && name == "AdapterError" {
                continue;
            }
            cases.push((format!("{name}:{label}"),"E0277",format!("use marmish_mdk::{name};\nfn require<T: {bound}>() {{}}\nfn main() {{ require::<{name}>(); }}")));
        }
    }
    let mut negative = json!({});
    for (case, expected, code) in cases {
        let file = work.join("negative.rs");
        fs::write(&file, code + "\n")?;
        let compiled = rust()
            .arg("--error-format=json")
            .arg(file)
            .arg("-o")
            .arg(&binary)
            .output()?;
        let mut codes = BTreeSet::new();
        for line in std::str::from_utf8(&compiled.stderr)?.lines() {
            let row: Value = serde_json::from_str(line)?;
            if let Some(code) = row["code"]["code"].as_str() {
                codes.insert(code.to_owned());
            }
        }
        require(
            !compiled.status.success() && codes.iter().map(String::as_str).eq([expected]),
            format!("unexpected diagnostics for {case}: {codes:?}"),
        )?;
        negative[case] = expected.into();
    }
    Ok((transcript, negative))
}
