//! Compose independent local IPC, parser, and argument validation observations.
use crate::{
    Result, capture, json_bytes, list, load, portable, require, run, sha, status_peer, stdout,
    whole,
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Profile {
    Status,
    Typed,
}

fn guard(
    binary: &Path,
    cmd: &mut Command,
    home: &Path,
    status: i32,
    diagnostic: &str,
) -> Result<()> {
    let result = capture(
        cmd.current_dir(home)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C")
            .env("HOME", home)
            .env("TMPDIR", home),
        5,
        None,
    )?;
    require(
        result.status.code() == Some(status)
            && result.stdout.is_empty()
            && result.stderr == diagnostic.as_bytes(),
        format!(
            "unexpected status guard response from {}: {cmd:?}",
            binary.display()
        ),
    )
}
fn guards(original: &Path, candidate: &Path) -> Result<(usize, usize)> {
    let tmp = tempfile::tempdir_in("/tmp")?;
    let home = tmp.path().canonicalize()?;
    let socket = home.join("unopened.sock");
    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "--foreground",
                "--max-commands",
                "1",
                "--new-session",
                "--tor",
                "invalid",
            ],
            "foreground_operation",
        ),
        (
            &["--max-commands", "1", "--new-session"],
            "server_limit_operation",
        ),
        (
            &["--max-uptime", "1h", "--new-session"],
            "server_limit_operation",
        ),
        (
            &["--new-session", "--tor", "invalid"],
            "new_session_operation",
        ),
        (&["--tor", "invalid"], "route_mode"),
    ];
    for &(args, label) in cases {
        for binary in [original, candidate] {
            guard(
                binary,
                Command::new(binary)
                    .arg("--state-dir")
                    .arg(&home)
                    .arg("--socket")
                    .arg(&socket)
                    .arg("status")
                    .args(args),
                &home,
                1,
                &format!("marmish: invalid input: {label}\n"),
            )?;
        }
    }
    let gaps = [
        ("--socket", &socket, "state directory"),
        ("--state-dir", &home, "socket"),
    ];
    for (flag, path, label) in &gaps {
        guard(
            candidate,
            Command::new(candidate).arg("status").arg(flag).arg(path),
            &home,
            125,
            &format!("marmish: explicit {label} required by this contract\n"),
        )?;
    }
    Ok((cases.len(), gaps.len()))
}

fn differences(original: &Path, candidate: &Path) -> Result<Value> {
    let cases = json!({"positional-response":[true,null,null],"non-null-command":{"ok":true,"command":{"state":"local_busy","extra":1}},"non-null-stream":{"ok":true,"stream":{"state":"pending"}}});
    let first = status_peer::replay(original, &cases)?;
    let second = status_peer::replay(candidate, &cases)?;
    let mut rows = json!({});
    for (label, suffix) in [
        ("positional-response", ""),
        ("non-null-command", r#","command":{"state":"local_busy"}"#),
        ("non-null-stream", r#","stream":{"state":"pending"}"#),
    ] {
        let expected = format!("{{\"ok\":true,\"result\":null,\"error\":null{suffix}}}\n");
        require(
            first[label] == json!({"exit":0,"stdout":expected,"stderr":""})
                && second[label]
                    == json!({"exit":1,"stdout":"","stderr":"marmish: invalid input: ipc_frame_json\n"}),
            format!("difference fixture changed at {label}"),
        )?;
        rows[label] = json!({"original":first[label],"candidate":second[label]});
    }
    Ok(rows)
}

fn verifier(name: &str) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "+1.96.1",
        "run",
        "--quiet",
        "--locked",
        "--offline",
        "--manifest-path",
    ])
    .arg(whole().join("canonical-public-data/Cargo.toml"))
    .args(["--bin", name, "--"]);
    cmd
}

pub fn qualify(
    profile: Profile,
    original_bins: &Path,
    generated_bins: &Path,
    receipt: &Path,
    framework: &str,
) -> Result<()> {
    let original_bins = original_bins.canonicalize()?;
    let generated_bins = generated_bins.canonicalize()?;
    let status = whole().join("status");
    let original = original_bins.join("marmish");
    let candidate = generated_bins.join("marmish");
    let digest = sha(&candidate)?;
    let mut metadata = json!({"frameworkRevision":framework});
    let shape = match profile {
        Profile::Status => {
            require(
                stdout(Command::new("git").current_dir(&status).args([
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                ]))?
                .is_empty(),
                "companion qualification requires a clean source revision",
            )?;
            metadata["companionRevision"] = json!(
                stdout(
                    Command::new("git")
                        .current_dir(&status)
                        .args(["rev-parse", "HEAD"])
                )?
                .trim()
            );
            metadata["cleanCompanion"] = json!(true);
            metadata["platform"] = json!("aarch64-apple-darwin");
            metadata["workspaceManifestSha256"] =
                json!(sha(&portable().join("native-status-workspace.json"))?);
            let shape = portable().join("native-status-v1");
            metadata["workspaceLockSha256"] = json!(sha(&shape.join("workspace.lock"))?);
            shape
        }
        Profile::Typed => {
            let reference = load(&status.join("macos-arm64.json"))?;
            require(
                reference["mode"] == "original",
                "expected original native baseline",
            )?;
            for role in ["marmish", "marmishd"] {
                require(
                    reference["originalBinaries"][role] == sha(&original_bins.join(role))?,
                    "original native executables differ",
                )?;
            }
            metadata["packageManifestSha256"] =
                json!(sha(&portable().join("native-status-typed-package.json"))?);
            let shape = portable().join("native-status-typed-v1");
            metadata["replyObligationsSha256"] = json!(sha(&shape.join("reply-obligations.json"))?);
            shape
        }
    };
    let tmp = tempfile::tempdir()?;
    let mut evidence = crate::status_reference::observe(
        &original_bins, Some(&candidate), &whole().join("bridge/aarch64-apple-darwin.json"),
    )?;
    require(
        evidence["candidateSha256"] == digest,
        "independent observation did not bind the generated client",
    )?;
    let (checks, gaps) = guards(&original, &candidate)?;
    metadata["orderedGuardsCompared"] = json!(checks);
    metadata["uncoveredLanes"] = json!(gaps);
    let fixture = whole().join("cli/transcript.json");
    run(verifier("cli-transcript")
        .arg("verify")
        .arg(&generated_bins)
        .arg("--fixture")
        .arg(&fixture))?;
    let transcript = load(&fixture)?;
    metadata["parserObservations"] = json!({"marmish":list(&transcript["marmish"])?.len(),"marmishd":list(&transcript["marmishd"])?.len()});
    metadata["graphSha256"] = json!(sha(&shape.join("graph.json"))?);
    metadata["packageLockSha256"] = json!(sha(&shape.join("package.lock"))?);
    if profile == Profile::Typed {
        for (corpus, report, archive) in [
            (
                "extended",
                "extendedReplies",
                "reply-shapes-macos-arm64.json",
            ),
            (
                "nested",
                "nestedReplies",
                "typed-reply-variants-macos-arm64.json",
            ),
        ] {
            let path = tmp.path().join(report);
            run(verifier("status-replies")
                .arg(corpus)
                .arg(&candidate)
                .arg(&path)
                .arg("--compare")
                .arg(status.join(archive)))?;
            let result = load(&path)?;
            require(
                result["binarySha256"] == digest,
                "observations use different binaries",
            )?;
            metadata[report] = result;
        }
        metadata["qualificationScope"] =
            json!("native client with explicit state and socket arguments");
    } else {
        metadata["observedDifferences"] = differences(&original, &candidate)?;
        metadata["unqualified"] = json!([
            "default socket discovery and profile validation",
            "installed executable validation, process-limit initialization and private startup routing",
            "the production-length status deadline against a stalled peer",
            "native status server generation and other command effects",
            "non-null typed command and stream replies",
            "typed tuple and list representations outside the observed JSON status result",
            "different-UID peer behavior in a mixed-process acceptance run"
        ]);
    }
    evidence
        .as_object_mut()
        .ok_or("invalid observed receipt")?
        .extend(metadata.as_object().ok_or("invalid metadata")?.clone());
    evidence.sort_all_objects();
    fs::write(receipt, json_bytes(&evidence)?)?;
    Ok(())
}
