mod members;
use core_qualification::*;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn callers(root: &Path, work: &Path, host: &str) -> Result<Value> {
    let target = work.join("target");
    let raw = output(
        Command::new("cargo")
            .current_dir(root)
            .args([
                "+1.97.1",
                "check",
                "--release",
                "--offline",
                "--locked",
                "--lib",
                "-p",
                "marmish-core",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(&target)
            .arg("--message-format=json"),
    )?;
    let records = raw
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    let compiled = one(records.iter().filter(|v| {
        v["reason"] == "compiler-artifact"
            && v["target"]["name"] == "marmish_core"
            && v["target"]["kind"] == json!(["lib"])
    }))?;
    let metadata = Path::new(one(arr(&compiled["filenames"])?
        .iter()
        .filter_map(Value::as_str)
        .filter(|f| f.ends_with(".rmeta")))?);
    let source = fs::read_to_string(whole().join("core-inherent/caller.rs"))?;
    let mut result = json!({});
    let controls = [
        ("complete", source.clone(), None),
        (
            "absent",
            source.replace("Retention::MAX_SECONDS", "Retention::UNDECLARED"),
            Some("E0599"),
        ),
        (
            "wrong-type",
            source.replace("const _: u64", "const _: u8"),
            Some("E0308"),
        ),
    ];
    for (key, text, expected) in controls {
        let probe = work.join("probe.rs");
        fs::write(&probe, text)?;
        let mut cmd = Command::new("rustc");
        cmd.args([
            "+1.97.1",
            "--edition=2021",
            "--target",
            host,
            "--crate-type=lib",
            "--emit=metadata",
            "--error-format=json",
            "--extern",
        ])
        .arg(format!("marmish_core={}", metadata.display()));
        for dir in [
            metadata
                .parent()
                .ok_or("missing artifact parent")?
                .to_owned(),
            target.join("release/deps"),
            target.join(host).join("release/deps"),
        ] {
            cmd.arg("-L").arg(format!("dependency={}", dir.display()));
        }
        let outcome = cmd
            .arg(probe)
            .arg("-o")
            .arg(work.join("caller.rmeta"))
            .output()?;
        let diagnostics = String::from_utf8(outcome.stderr)?;
        let records = diagnostics
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<Value>, _>>()?;
        let errors: Vec<_> = records
            .iter()
            .filter(|r| r["level"] == "error")
            .filter_map(|r| r["code"]["code"].as_str())
            .collect();
        require(
            outcome.status.success() == expected.is_none()
                && errors == expected.into_iter().collect::<Vec<_>>(),
            format!("unexpected diagnostics for {key}: {diagnostics}"),
        )?;
        result[key] = json!(errors);
    }
    Ok(result)
}

const USAGE: &str = "usage: core-inherent FRAMEWORK ORIGINAL CANDIDATE OUTPUT";

fn main() -> Result {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(Into::into).collect();
    if args
        .iter()
        .any(|arg| arg.as_os_str() == "--help" || arg.as_os_str() == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    let [framework, original, candidate, dest] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let original = original.canonicalize()?;
    let candidate = candidate.canonicalize()?;
    pinned_git(&original, ORIGINAL)?;
    let seal = snapshot(&candidate)?;
    let version = output(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing toolchain host")?;
    let directory = portable().join("core-structure-v1");
    let member_file = directory.join("members.ttl");
    let (paths, members) = members::checked(&directory)?;
    let archive = lexical().join(host);
    let archived_receipt = load(&archive.join("receipt.json"))?;
    let observer = Observer::new(&framework.canonicalize()?, INHERENT_OBSERVER)?;
    let mut results = json!({});
    for (n, (label, root)) in [("original", &original), ("generated", &candidate)]
        .into_iter()
        .enumerate()
    {
        let temp = tempfile::tempdir()?;
        let report = temp.path().join("report.json");
        observer.capture(root, host, &report, n == 0, true)?;
        let mut data = load(&report)?;
        let observed = data
            .as_object_mut()
            .ok_or("invalid report")?
            .remove("rustdocPublicInherent")
            .ok_or("missing inherent observations")?;
        let previous_file = archive.join(format!("{label}.json"));
        require(
            sha(&previous_file)? == txt(&archived_receipt["reportsSha256"][label])?,
            "archived core lexical report changed",
        )?;
        let previous = load(&previous_file)?;
        require(
            data == previous,
            "profile or underlying core declarations changed",
        )?;
        for (owner, pair) in &paths {
            let fields: Vec<_> = arr(&observed["items"])?
                .iter()
                .filter(|r| r["owner"] == pair[n])
                .map(|r| json!({"kind":r["kind"],"name":r["name"]}))
                .collect();
            require(
                members[owner] == json!(fields),
                format!("member declarations differ for {owner}"),
            )?;
        }
        results[label] = json!({"completeReportSha256":sha(&report)?,"publicInherent":observed,"caller":callers(root,temp.path(),host)?});
    }
    observer.check()?;
    require(snapshot(&candidate)? == seal, "generated input changed")?;
    pinned_git(&original, ORIGINAL)?;
    save(
        dest,
        &json!({"originalRevision":ORIGINAL,"observerRevision":observer.revision,"host":host,"inputSeal":seal,"members":members,"results":results,
        "callerSha256":sha(&whole().join("core-inherent/caller.rs"))?,"memberGraphSha256":sha(&member_file)?,"swappedEncodeRoleRejected":true}),
    )
}
