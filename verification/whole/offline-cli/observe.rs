//! Compare the original and both generated parsers through a read-only Darwin process.
use public_data_qualification::{
    Result, assembly::DARWIN, capture, json_bytes, list, load, portable, require, sha, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::Command,
};

const ROLES: [&str; 2] = ["marmish", "marmishd"];
const POLICY: &str =
    "(version 1) (allow default) (deny network*) (deny file-write*) (deny process-fork)";

pub fn observe(binary: &Path, home: &Path, args: &[String]) -> Result<Value> {
    let home = home.canonicalize()?;
    let mut child = Command::new("/usr/bin/sandbox-exec");
    child
        .args(["-p", POLICY])
        .arg(binary.canonicalize()?)
        .args(args)
        .current_dir(&home)
        .env_clear()
        .env("HOME", &home)
        .env("LANG", "C")
        .env("PATH", "/usr/bin:/bin");
    // umask is async-signal-safe and changes only the child.
    unsafe {
        child.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    let output = capture(&mut child, 5, None)?;
    require(
        output.stdout.is_ascii() && output.stderr.is_ascii(),
        "non-ASCII CLI output",
    )?;
    let exit = output
        .status
        .code()
        .or_else(|| output.status.signal().map(|n| -n))
        .ok_or("missing exit status")?;
    Ok(
        json!({"args":args,"exit":exit,"stdout":String::from_utf8(output.stdout)?,"stderr":String::from_utf8(output.stderr)?}),
    )
}

fn historical(args: &[String]) -> bool {
    match args {
        [single] => matches!(
            single.as_str(),
            "--help" | "--version" | "--oddlang-unrecognized-option"
        ),
        [help, name] => {
            help == "help"
                && name.bytes().any(|b| b.is_ascii_alphanumeric())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }
        _ => false,
    }
}

pub fn compare(
    original: &Path,
    baseline: &Path,
    baseline_build: &Path,
    successor: &Path,
    successor_build: &Path,
    receipt: &Path,
) -> Result {
    require(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "offline comparison requires macOS ARM64",
    )?;
    let seven = whole()
        .join("unified-buffered")
        .join(format!("{DARWIN}.json"));
    let cli = whole().join("cli/transcript.json");
    let provenance = load(&seven)?;
    let history = load(&cli)?;
    let cases: Vec<Vec<String>> = serde_json::from_slice(include_bytes!("cases.json"))?;
    let builds = [
        (
            "baseline",
            baseline_build,
            "workspace-v7-recovered-vault-engine-journal-mdk-buffered",
        ),
        ("successor", successor_build, "workspace-v7-offline-cli"),
    ];
    let inputs = [
        ("original", original),
        ("baseline", baseline),
        ("successor", successor),
    ];
    let mut hashes = json!({});
    for (side, dir) in inputs {
        for role in ROLES {
            hashes[side][role] = sha(&dir.join(role))?.into();
        }
    }
    require(
        hashes["original"] == provenance["releaseInfo"]["originalBinaries"],
        "original binaries differ from sealed release",
    )?;
    let mut receipts = json!({});
    for (side, path, manifest) in builds {
        let data = load(path)?;
        let expected = if side == "baseline" {
            provenance["framework"]
                .as_str()
                .ok_or("missing framework")?
        } else {
            "0fc48e920e30baf6396218cee0983732ba6c3a78"
        };
        require(
            data["binarySha256"] == hashes[side]
                && data["frameworkRevision"] == expected
                && data["workspaceManifestSha256"]
                    == sha(&portable().join(format!("{manifest}.json")))?
                && data["workspaceLockSha256"]
                    == sha(&portable().join(manifest).join("workspace.lock"))?,
            format!("build or binary pin differs: {side}"),
        )?;
        receipts[side] = data;
    }
    let home = tempfile::tempdir()?;
    let mut counts = json!({});
    let mut observed = json!({});
    for role in ROLES {
        let archive = list(&history[role])?;
        counts[role] = archive.len().into();
        let mut rows = vec![json!({}); cases.len()];
        for (side, dir) in inputs {
            let binary = dir.join(role);
            for row in archive {
                let args: Vec<String> = serde_json::from_value(row["args"].clone())?;
                require(historical(&args), "unexpected historical invocation")?;
                require(
                    observe(&binary, home.path(), &args)? == *row,
                    format!("changed historical transcript in {side}/{role}: {args:?}"),
                )?;
            }
            for (row, args) in rows.iter_mut().zip(&cases) {
                row[side] = observe(&binary, home.path(), args)?;
            }
        }
        for (side, expected_gaps) in [("baseline", 6), ("successor", 2)] {
            let mismatches = rows
                .iter()
                .zip(&cases)
                .filter(|(r, _)| r[side] != r["original"])
                .map(|(_, a)| a);
            let first_gap = cases
                .len()
                .checked_sub(expected_gaps)
                .ok_or("incomplete CLI cases")?;
            require(
                mismatches.eq(cases[first_gap..].iter()),
                format!("unexpected parser compatibility results for {role}/{side}"),
            )?;
        }
        observed[role] = rows.into();
    }
    require(
        fs::read_dir(home.path())?.next().is_none(),
        "parse-only audit changed working directory",
    )?;
    let data = json!({"platform":provenance["platform"],"originalRevision":provenance["original"],"baselineInputRevision":provenance["companion"],
        "baselineFramework":provenance["framework"],"baselineWorkspaceLockSha256":provenance["workspaceLockSha256"],"originalReleaseReceiptSha256":sha(&seven)?,
        "historicalTranscriptSha256":sha(&cli)?,"historicalRowsPerBinary":counts,"binarySha256":hashes,"builds":receipts,"rows":observed});
    fs::write(receipt, json_bytes(&data)?)?;
    for role in ROLES {
        let rows = list(&observed[role])?;
        let matches = |side: &str| rows.iter().filter(|r| r[side] == r["original"]).count();
        println!(
            "{role} {{'baseline': {}, 'successor': {}}} / {}",
            matches("baseline"),
            matches("successor"),
            rows.len()
        );
    }
    Ok(())
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Profile {
    Prefix,
    LinuxHelp,
}

fn help_or_version(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "help")
        || args
            .last()
            .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h" | "--version" | "-V"))
}

pub fn terminal_arguments(profile: Profile, role: &str) -> Result<Value> {
    require(ROLES.contains(&role), "unknown terminal role")?;
    let transcript = load(&whole().join("cli/transcript.json"))?;
    let mut cases = Vec::new();
    let selected = |args: &[String]| matches!(profile, Profile::Prefix) || help_or_version(args);
    for row in list(&transcript[role])? {
        let args: Vec<String> = serde_json::from_value(row["args"].clone())?;
        require(!args.is_empty(), "empty historical argument vector")?;
        if selected(&args) {
            require(
                historical(&args),
                format!("unexpected historical invocation: {args:?}"),
            )?;
            cases.push(args);
        }
    }
    let offline: Vec<Vec<String>> = serde_json::from_slice(include_bytes!("cases.json"))?;
    cases.extend(offline.into_iter().filter(|args| selected(args)));
    match profile {
        Profile::LinuxHelp => Ok(json!({ "cases": cases })),
        Profile::Prefix => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Prefix {
                additional: Vec<Vec<String>>,
                post_parse: Vec<String>,
            }
            let prefix: Prefix =
                serde_json::from_slice(include_bytes!("../terminal-prefix/arguments.json"))?;
            cases.extend(prefix.additional);
            Ok(json!({ "cases": cases, "postParse": prefix.post_parse }))
        }
    }
}
