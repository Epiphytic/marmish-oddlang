//! Original-source tests reused by standalone and workspace vault qualifiers.
use crate::{assembly::*, *};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub fn reference(
    member: &Path,
    lock: &Path,
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
    fs::copy(lock, dir.join("Cargo.lock"))?;
    let lib = serde_json::to_string(member)?;
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
pub fn link_command(source: &Path, library: &Path, deps: &Path, out: &Path, test: bool) -> Command {
    let mut rust = Command::new("rustc");
    rust.args(["+1.97.1", "--edition=2021"]);
    if test {
        rust.args(["--test", "--error-format=json"]);
    }
    rust.arg(source)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("marmish_vault={}", library.display()))
        .arg("-o")
        .arg(out);
    rust
}

pub fn link(source: &Path, library: &Path, deps: &Path, out: &Path, test: bool) -> Result {
    run(&mut link_command(source, library, deps, out, test))?;
    Ok(())
}
pub const STRICT: &str = "tests::strict_public_values";
pub const MODULE: &str = "5389218be38e60288371fe63a79ae71d2e9dbb8408a34bc53e8010f03117359c";
pub fn strict(
    original: &Path,
    target: &Path,
    host: &str,
    work: &Path,
    library: &Path,
    deps: &Path,
) -> Result<PathBuf> {
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
    require(digest == MODULE, "unexpected upstream test module")?;
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
                    STRICT,
                ]),
        )?,
        STRICT,
    )?;
    let harness = work.join("strict-vault-values.rs");
    fs::write(&harness, format!("pub use marmish_vault::*;\n{module}"))?;
    let strict_bin = work.join("probe-strict-vault-values");
    link(&harness, library, deps, &strict_bin, true)?;
    passed(
        stdout(Command::new(strict_bin).args(["--exact", STRICT]))?,
        STRICT,
    )?;
    Ok(harness)
}
pub fn holdout(original: &Path, work: &Path, library: &Path, deps: &Path) -> Result<Value> {
    let test = "from_bytes_owns_the_input_and_requires_drop";
    let hold = whole().join("unified-recovered-vault/fixed_bytes.rs");
    let dir = work.join("reference-from-bytes");
    reference(
        &original.join("crates/marmish-vault"),
        &original.join("Cargo.lock"),
        &dir,
        "observe-vault-from-bytes",
        &hold,
        true,
    )?;
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
    let binary = work.join("generated-from-bytes");
    link(&hold, library, deps, &binary, true)?;
    passed(stdout(Command::new(binary).args(["--exact", test]))?, test)?;
    Ok(json!({"name":test,"sourceSha256":sha(&hold)?,"originalAndGeneratedPassed":true}))
}

const FAULT: &str = "Malformed/malformed/malformed";
pub fn expected(maximum: usize) -> Vec<String> {
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
                && s.len() <= maximum
                && !s
                    .chars()
                    .any(|c| matches!(c as u32,0..=31|127..=159|0x2028..=0x202e|0x2066..=0x2069)),
        ));
    }
    rows.push(format!("Metadata {{ key_ref: KeyRef {{ version: 2, purpose: Sqlcipher, id: \"{}\" }}, label: Label(\"local-index\") }}","a".repeat(64)));
    rows
}

pub fn capture_shapes(tool: &Path, member: &Path, host: &str) -> Result<Vec<u8>> {
    run(Command::new(tool)
        .env("ODD_RECOVERY_DATA_SHAPES", "1")
        .arg(member.join("Cargo.toml"))
        .args(["marmish-vault", "lib", "marmish_vault", host, "default"]))
}
