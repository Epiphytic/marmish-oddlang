//! Independent native vault corpus and unchanged original strict/holdout tests.
use public_data_qualification::{assembly::*, *};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
const FAULT: &str = "Malformed/malformed/malformed";
fn expected() -> Vec<String> {
    let mut rows: Vec<String> = [
        "Legacy/1",
        "MarmishV2/2",
        "Nostr/nostr",
        "Sqlcipher/sqlcipher",
    ]
    .map(str::to_owned)
    .into();
    rows.extend((0..6).map(|_| FAULT.to_string()));
    for (n, c) in [
        ("Unavailable", "unavailable"),
        ("Uninitialized", "uninitialized"),
        ("LockedOrDenied", "locked_or_denied"),
        ("Missing", "missing"),
        ("Exists", "exists"),
        ("Ambiguous", "ambiguous"),
        ("Malformed", "malformed"),
        ("BackendFailure", "backend_failure"),
    ] {
        rows.push(format!("{n}/{c}/{c}"));
    }
    let verdict = |ok| if ok { "ok".into() } else { FAULT.to_owned() };
    rows.extend((0..256).map(|v| verdict(v == 1 || v == 2)));
    rows.extend([0, 1, 63, 64, 65].map(|n| verdict(n == 64)));
    rows.extend((0..128u8).map(|c| verdict(b"0123456789abcdef".contains(&c))));
    rows.extend([FAULT.to_string(), FAULT.to_string()]);
    let mut labels: Vec<String> = [0, 1, 126, 127, 128, 129].map(|n| "a".repeat(n)).into();
    labels.extend([
        "é".repeat(64),
        "🦀".repeat(32),
        "a".repeat(125) + "🦀",
        "a".repeat(126) + "é",
        "a".repeat(127) + "é",
        "a".repeat(128) + "\u{1f}",
        "\u{200b}".into(),
        "\u{feff}".into(),
    ]);
    for n in (0..0xa1).chain(0x2027..0x2030).chain(0x2065..0x206b) {
        labels.push(format!("A{}B", char::from_u32(n).unwrap()));
    }
    for s in labels {
        rows.push(verdict(
            !s.is_empty()
                && s.len() <= 128
                && !s
                    .chars()
                    .any(|c| matches!(c as u32,0..=31|127..=159|0x2028..=0x202e|0x2066..=0x2069)),
        ));
    }
    rows.push(format!("Metadata {{ key_ref: KeyRef {{ version: 2, purpose: Sqlcipher, id: \"{}\" }}, label: Label(\"local-index\") }}","a".repeat(64)));
    rows
}
fn reference(
    original: &Path,
    dir: &Path,
    package: &str,
    original_source: &Path,
    test: bool,
) -> Result {
    fs::create_dir_all(dir.join("src"))?;
    fs::copy(
        original_source,
        dir.join(if test { "src/lib.rs" } else { "src/main.rs" }),
    )?;
    fs::copy(original.join("Cargo.lock"), dir.join("Cargo.lock"))?;
    let lib = serde_json::to_string(&original.join("crates/marmish-vault"))?;
    let version = if test { "0.0.0" } else { "0.1.0" };
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname={package:?}\nversion={version:?}\nedition=\"2021\"\n[dependencies]\nmarmish-vault={{path={lib}}}\n[workspace]\n"
        ),
    )?;
    let meta: Value = serde_json::from_slice(&run(Command::new("cargo")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .args(["+1.97.1", "metadata", "--offline", "--format-version", "1"])
        .current_dir(dir))?)?;
    let vault: Vec<_> = list(&meta["packages"])?
        .iter()
        .filter(|r| r["name"] == "marmish-vault")
        .collect();
    require(
        vault.len() == 1 && vault[0]["version"] == "0.1.0" && vault[0]["edition"] == "2021",
        "unexpected reference vault package",
    )
}
fn passed(raw: String, name: &str) -> Result {
    require(
        raw.lines()
            .filter(|l| *l == format!("test {name} ... ok"))
            .count()
            == 1
            && raw
                .lines()
                .any(|l| l.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;")),
        "expected exactly one unchanged test pass",
    )
}
fn link(source: &Path, library: &Path, deps: &Path, out: &Path, test: bool) -> Result {
    let mut rust = Command::new("rustc");
    rust.args(["+1.97.1", "--edition=2021"]);
    if test {
        rust.args(["--test", "--error-format=json"]);
    }
    run(rust
        .arg(source)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("marmish_vault={}", library.display()))
        .arg("-o")
        .arg(out))?;
    Ok(())
}
pub fn observe(
    original: &Path,
    target: &Path,
    host: &str,
    directories: (&Path, &Path),
    library: &Path,
    workspace: &Value,
    predecessor: Option<&Path>,
) -> Result<Value> {
    let (work, scratch) = directories;
    let inputs = work.join("inputs");
    let deps = work.join("product/target/release/deps");
    let caller = whole().join("vault-values/caller.rs");
    let binary = scratch.join("vault-public-values");
    link(&caller, library, &deps, &binary, false)?;
    let original_caller = scratch.join("vault-original");
    reference(
        original,
        &original_caller,
        "observe-vault-values",
        &caller,
        false,
    )?;
    run(Command::new("cargo")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .args(["+1.97.1", "build", "--release", "--offline", "--locked"])
        .current_dir(&original_caller))?;
    let old = stdout(&mut Command::new(
        original_caller.join("target/release/observe-vault-values"),
    ))?;
    let new = stdout(&mut Command::new(binary))?;
    let rows = expected();
    require(
        new.lines().eq(old.lines()) && new.lines().eq(rows.iter().map(String::as_str)),
        "sealed vault public values differ",
    )?;
    let strict = "tests::strict_public_values";
    let src = stdout(Command::new("git").current_dir(original).args([
        "show",
        &format!("{ORIGINAL}:crates/marmish-vault/src/lib.rs"),
    ]))?;
    let marker = "#[cfg(test)]\nmod tests {";
    require(
        src.matches(marker).count() == 1,
        "ambiguous original test module",
    )?;
    let module = &src[src.find(marker).unwrap()..];
    let digest = bytes_sha(module.as_bytes());
    require(
        digest == "5389218be38e60288371fe63a79ae71d2e9dbb8408a34bc53e8010f03117359c",
        "unexpected upstream test module",
    )?;
    passed(
        stdout(
            Command::new("cargo")
                .env_remove("CARGO_TARGET_DIR")
                .env_remove("CARGO_BUILD_TARGET")
                .current_dir(original)
                .args([
                    "+1.97.1",
                    "test",
                    "--locked",
                    "--offline",
                    "--release",
                    "--target",
                    host,
                    "--target-dir",
                ])
                .arg(target)
                .args([
                    "--lib",
                    "--package",
                    "marmish-vault",
                    "--",
                    "--exact",
                    strict,
                ]),
        )?,
        strict,
    )?;
    let harness = work.join("strict-vault-values.rs");
    fs::write(&harness, format!("pub use marmish_vault::*;\n{module}"))?;
    let strict_bin = work.join("probe-strict-vault-values");
    link(&harness, library, &deps, &strict_bin, true)?;
    passed(
        stdout(Command::new(strict_bin).args(["--exact", strict]))?,
        strict,
    )?;
    let receipt = whole().join("vault-fixed-bytes/aarch64-apple-darwin.json");
    let prior = load(&receipt)?;
    let package = &workspace["members"]["marmish-vault"]["manifest"];
    let vault = load(&inputs.join(text(&package["path"])?))?;
    let values: Vec<_> = list(&vault["components"])?
        .iter()
        .filter(|c| {
            c["reexports"]
                .as_array()
                .is_some_and(|r| r.iter().any(|n| n == "Secret"))
        })
        .collect();
    let [value] = values.as_slice() else {
        return Err("expected a fixed vault component".into());
    };
    let package_hash = if let Some(p) = predecessor {
        let mut baseline = load(p)?;
        let mut successor = vault.clone();
        let inherited = baseline
            .as_object_mut()
            .and_then(|v| v.shift_remove("components"))
            .ok_or("missing predecessor components")?;
        let components = successor
            .as_object_mut()
            .and_then(|v| v.shift_remove("components"))
            .ok_or("missing successor components")?;
        let inherited = list(&inherited)?;
        let components = list(&components)?;
        require(
            baseline == successor
                && components.len() > inherited.len()
                && components.starts_with(inherited),
            "vault predecessor differs beyond appended components",
        )?;
        sha(p)?
    } else {
        text(&package["sha256"])?.to_owned()
    };
    require(
        prior["original"] == ORIGINAL
            && prior["framework"] == "595fb7a55dc2016cfab70c5b131bf2f6acfe9983"
            && prior["unchangedModuleSha256"] == digest
            && prior["graphSha256"] == value["graph"]["sha256"]
            && prior["packageManifestSha256"] == package_hash,
        "fixed-value package differs from exact native precedent",
    )?;
    let test = "from_bytes_owns_the_input_and_requires_drop";
    let hold = whole().join("unified-recovered-vault/fixed_bytes.rs");
    let dir = work.join("reference-from-bytes");
    reference(original, &dir, "observe-vault-from-bytes", &hold, true)?;
    passed(
        stdout(
            Command::new("cargo")
                .env_remove("CARGO_TARGET_DIR")
                .env_remove("CARGO_BUILD_TARGET")
                .current_dir(&dir)
                .args([
                    "+1.97.1",
                    "test",
                    "--locked",
                    "--offline",
                    "--release",
                    "--lib",
                    "--",
                    "--exact",
                    test,
                ]),
        )?,
        test,
    )?;
    let binary = scratch.join("generated-from-bytes");
    link(&hold, library, &deps, &binary, true)?;
    passed(stdout(Command::new(binary).args(["--exact", test]))?, test)?;
    Ok(json!({
        "name": strict,
        "unchangedModuleSha256": digest,
        "originalAndGeneratedPassed": true,
        "standaloneReceiptSha256": sha(&receipt)?,
        "fromBytesHoldout": {"name":test,
        "sourceSha256": sha(&hold)?,
        "originalAndGeneratedPassed": true},
        "librarySha256": sha(library)?,
        "publicValueRows": rows.len(),
    }))
}
