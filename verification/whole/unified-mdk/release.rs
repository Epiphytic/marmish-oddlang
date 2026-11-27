//! Exercise both native release-info roles with the sixteen original invocations.
use public_data_qualification::{assembly::ORIGINAL, *};
use serde_json::{Value, json};
use std::{fs, os::unix::process::CommandExt, path::Path, process::Command};
const CASES: [&[&str]; 16] = [
    &["release-info"],
    &["--state-dir", "absent", "release-info"],
    &["release-info", "--state-dir", "absent"],
    &[
        "--key-storage",
        "file",
        "--tor",
        "off",
        "--relay",
        "unused",
        "release-info",
    ],
    &["--foreground", "release-info"],
    &["release-info", "--new-session"],
    &["--max-commands", "1", "release-info"],
    &["release-info", "--max-commands", "invalid"],
    &["release-info", "--max-uptime", "invalid"],
    &["--new-session", "--max-uptime", "1", "release-info"],
    &["--new-session", "--max-commands", "1", "release-info"],
    &[
        "--foreground",
        "--max-commands",
        "1",
        "--new-session",
        "release-info",
    ],
    &["release-info", "--not-an-option"],
    &["release-info", "--max-commands"],
    &["release-info", "extra"],
    &["release-info", "--help"],
];
fn capture(binary: &Path, argv: &[&str], home: &Path) -> Result<Value> {
    let mut cmd = Command::new(binary);
    cmd.args(argv)
        .current_dir(home)
        .env_clear()
        .env("HOME", home)
        .env("TMPDIR", home)
        .env("PATH", "/bin:/usr/bin")
        .env("LANG", "C");
    for k in [
        "CARGO_PKG_VERSION",
        "TARGET",
        "MARMISH_BUILD_COMMIT",
        "MARMISH_BUILD_TARGET",
    ] {
        cmd.env(k, "runtime-override");
    }
    // umask is async-signal-safe and executes only in the newly forked process.
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    let output = public_data_qualification::capture(&mut cmd, 30, None)?;
    Ok(
        json!({"exit":output.status.code().ok_or("release-info stopped by signal")?,
        "stdout":String::from_utf8(output.stdout)?, "stderr":String::from_utf8(output.stderr)?}),
    )
}
pub fn observe(
    original: &Path,
    portable: &Path,
    upstream: &Path,
    candidate: &Path,
    host: &str,
) -> Result<Value> {
    let graph = load(&portable.join("native-configured-init-v1/graph.json"))?;
    let mut facts = Vec::new();
    let mut formats = Vec::new();
    for (role, file, symbol, ty, generated) in [
        (
            "profile",
            "crates/marmish-native/src/identity.rs",
            "PROFILE_FORMAT",
            "u8",
            Some(&graph["manifest"]["version"]),
        ),
        (
            "journal",
            "crates/marmish-native/src/journal.rs",
            "JOURNAL_FORMAT",
            "i64",
            Some(&graph["initialize"]["journal"]["version"]),
        ),
        (
            "engine",
            "crates/marmish-engine/src/store.rs",
            "ENGINE_FORMAT",
            "i64",
            None,
        ),
        (
            "configuration",
            "crates/marmish-native/src/command_config.rs",
            "CONFIG_FORMAT",
            "u32",
            Some(&graph["initialize"]["configured"]["fixed"]["version"]),
        ),
        (
            "command",
            "crates/marmish-native/src/command_wire.rs",
            "COMMAND_FORMAT",
            "u32",
            None,
        ),
    ] {
        let path = original.join(file);
        let raw = fs::read_to_string(&path)?;
        let re = regex::Regex::new(&format!(r"\bconst {symbol}: {ty} = ([0-9]+);"))?;
        let values: Vec<u32> = re
            .captures_iter(&raw)
            .map(|m| m[1].parse())
            .collect::<std::result::Result<_, _>>()?;
        let [value] = values.as_slice() else {
            return Err(format!("expected one format constant for {symbol}").into());
        };
        require(
            (ty != "u8" || *value < 256) && generated.is_none_or(|declared| *declared == *value),
            "generated format differs from original",
        )?;
        formats.push(*value);
        facts.push(json!({"role":role,"source":file,"sourceSha256":sha(&path)?,"identifier":symbol,"declaredType":ty,"value":value,"generatedValue":generated}));
    }
    let manifest: toml::Table = toml::from_str(&fs::read_to_string(
        original.join("crates/marmish-native/Cargo.toml"),
    )?)?;
    let version = manifest["package"]["version"]
        .as_str()
        .ok_or("missing native version")?;
    let expected =
        json!({"schema":1,"version":version,"target":host,"commit":ORIGINAL,"formats":formats})
            .to_string()
            + "\n";
    let temp = tempfile::tempdir()?;
    let mut reports = json!({});
    let mut old = json!({});
    let mut new = json!({});
    for role in ["marmish", "marmishd"] {
        let mut cases = Vec::new();
        for (i, argv) in CASES.iter().enumerate() {
            let reference = capture(&upstream.join(role), argv, temp.path())?;
            let candidate = capture(&candidate.join(role), argv, temp.path())?;
            require(
                reference == candidate,
                format!("release-info diverged for {role} {argv:?}"),
            )?;
            if i < 4 {
                require(
                    reference == json!({"exit":0,"stdout":expected,"stderr":""}),
                    format!("unexpected build identity for {role}"),
                )?;
            }
            cases.push(json!({"argv":argv,"exit":reference["exit"],"stdout":reference["stdout"],"stderr":reference["stderr"]}));
        }
        reports[role] = cases.into();
        old[role] = sha(&upstream.join(role))?.into();
        new[role] = sha(&candidate.join(role))?.into();
    }
    require(
        fs::read_dir(temp.path())?.next().is_none(),
        "release-info created runtime state",
    )?;
    Ok(json!({"cases":reports,"formats":facts,"originalBinaries":old,"generatedBinaries":new}))
}
