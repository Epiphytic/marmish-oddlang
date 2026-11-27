//! Observe the published candidate's unprepared CLI operations with networking denied.
use clap::{Parser, ValueEnum};
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, seatbelt_policy},
    capture, json_bytes, list, portable, require, sha,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    #[arg(value_enum)]
    release: Release,
    export: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    build_receipt_sha256: String,
}
#[derive(Clone, Copy, ValueEnum)]
enum Release {
    Candidate,
    Prepare,
    PrepareFixed,
}

fn regular(path: &Path) -> Result {
    require(
        path.symlink_metadata()?.is_file(),
        format!("expected ordinary file: {}", path.display()),
    )
}

fn verify(export: &Path, digest: &str, release: Release) -> Result<(Value, Value)> {
    let receipt = export.join("receipt.json");
    regular(&receipt)?;
    let bytes = fs::read(receipt)?;
    require(
        bytes_sha(&bytes) == digest,
        "generated build receipt digest mismatch",
    )?;
    let build: Value = serde_json::from_slice(&bytes)?;
    require(
        build["format"] == "marmish-basic-command-generated-build-v1",
        "unexpected build format",
    )?;
    let mut pins = json!({});
    if !matches!(release, Release::Candidate) {
        require(
            build["platform"] == "aarch64-apple-darwin"
                && build["artifacts"]
                    .as_object()
                    .is_some_and(|obj| obj.len() == 2),
            "unexpected build platform or roles",
        )?;
        let probes = build["deniedReadProbes"]
            .as_object()
            .ok_or("missing read denials")?;
        require(
            [
                "originalSource",
                "originalBinary",
                "oracleFixture",
                "defaultDenyCanary",
            ]
            .iter()
            .all(|k| probes.contains_key(*k)),
            "missing build read denials",
        )?;
        let manifest = if matches!(release, Release::Prepare) {
            "workspace-bounded-owner-prepare.json"
        } else {
            "workspace-bounded-owner-prepare-fixed.json"
        };
        for relative in [
            manifest,
            "marmish-native-bounded-owner-prepare-package.json",
            "bounded-owner-prepare-v1/foreground.json",
            "bounded-owner-prepare-v1/status.rust.json",
        ] {
            let digest = sha(&portable().join(relative))?;
            require(
                build["stagedContracts"][relative] == digest,
                format!("composition pin differs: {relative}"),
            )?;
            pins[relative] = json!(digest);
        }
    }
    for role in ["marmish", "marmishd"] {
        let image = export.join(role);
        regular(&image)?;
        require(
            build["artifacts"][role]["sha256"] == sha(&image)?,
            format!("generated executable digest differs: {role}"),
        )?;
    }
    Ok((build, pins))
}

struct Case<'a> {
    name: &'a str,
    role: &'a str,
    args: &'a [&'a str],
    code: i32,
    error: &'a str,
}

fn main() -> Result {
    let args = Args::parse();
    let export = args.export.canonicalize()?;
    let (build, pins) = verify(&export, &args.build_receipt_sha256, args.release)?;
    let candidate = matches!(args.release, Release::Candidate);
    let work = tempfile::tempdir()?;
    let root = work.path().canonicalize()?;
    let policy = seatbelt_policy(&root, &[&export], &[])?;
    let unavailable = "command operation unavailable\n";
    let mut cases = Vec::new();
    if candidate {
        for (name, arguments) in [
            ("foregroundSubmit", &["audit-peer", "/usr/bin/true"][..]),
            (
                "detachedSubmit",
                &["--detach", "audit-peer", "/usr/bin/true"],
            ),
            ("wait", &["wait", "--latest", "audit-peer"]),
            ("authorize", &["keychain-authorize"]),
        ] {
            cases.push(Case {
                name,
                role: "marmish",
                args: arguments,
                code: 125,
                error: unavailable,
            });
        }
    }
    let setup_bound = !candidate
        || list(&build["cliSurfaces"]["marmish"]["boundOperations"])?
            .iter()
            .any(|s| s == "setup");
    cases.push(Case {
        name: if candidate {
            "setup"
        } else {
            "setupWithoutFileProvider"
        },
        role: "marmish",
        args: &["setup", "--role", "client", "--no-grants"],
        code: if setup_bound { 1 } else { 125 },
        error: if setup_bound {
            "marmish: profile preparation requires the file key provider\n"
        } else {
            unavailable
        },
    });
    cases.push(Case{name:if candidate{"daemonWithoutState"}else{"foregroundWithoutState"},role:"marmishd",args:&["--foreground"],code:125,error:"marmish: foreground status owner requires explicit state and a prepared file profile\n"});
    let mut rows = json!({});
    for case in cases {
        let scratch = if candidate {
            let dir = root.join(case.name);
            fs::create_dir(&dir)?;
            dir
        } else {
            root.clone()
        };
        let result = capture(
            Command::new("/usr/bin/sandbox-exec")
                .args(["-p", &policy])
                .arg(export.join(case.role))
                .args(case.args)
                .current_dir(&scratch)
                .env_clear()
                .env("HOME", &scratch)
                .env("TMPDIR", &scratch)
                .env("PATH", "/usr/bin:/bin")
                .env("LC_ALL", "C"),
            30,
            None,
        )?;
        require(
            result.status.code() == Some(case.code)
                && result.stdout.is_empty()
                && result.stderr == case.error.as_bytes(),
            format!("unexpected operation availability: {}", case.name),
        )?;
        require(
            fs::read_dir(&scratch)?.next().is_none(),
            format!("operation created state: {}", case.name),
        )?;
        let mut row = json!({"role":case.role,"argv":case.args,"exit":case.code,"stdout":"","stderr":case.error});
        if candidate {
            row["workspaceUnchanged"] = json!(true);
        }
        rows[case.name] = row;
    }
    work.close()?;
    require(!root.try_exists()?, "preflight workspace remains")?;
    let format = if candidate {
        "marmish-basic-command-generated-preflight-v1"
    } else {
        "marmish-bounded-room-prepare-preflight-v1"
    };
    let mut report = json!({"format":format,"buildReceiptSha256":args.build_receipt_sha256,"preflightSha256":bytes_sha(include_bytes!("preflight.rs")),"cases":rows,"workspaceRemoved":true});
    if candidate {
        report.sort_all_objects();
    } else {
        report["compositionPins"] = pins;
    }
    let parent = args
        .receipt
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(&json_bytes(&report)?)?;
    tmp.persist_noclobber(args.receipt)?;
    Ok(())
}
