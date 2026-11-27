//! Replay public stream observations using the pinned original compiler and native oracle tools.
use clap::Parser;
use public_data_qualification::{
    Result, assembly, baseline_framework, load, portable, require, run, sha, stdout, text, whole,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::Command,
};

const TOOLS: &str = "08373b871aeac0f76cb1675748fd604cea2a79a2";
const LOCK: &str = "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3";

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    upstream: PathBuf,
    output: PathBuf,
    #[arg(long)]
    tooling_checkout: PathBuf,
    #[arg(long)]
    holdout: Option<PathBuf>,
}

fn fields(current: &Value, archived: &Value, keys: &[&str]) -> Result {
    for &key in keys {
        require(
            archived.get(key).is_some() && current.get(key) == archived.get(key),
            format!("receipt differs at {key}"),
        )?;
    }
    Ok(())
}

fn remaining(current: &Value, archived: &Value, exceptions: &[&str]) -> Result {
    for key in archived
        .as_object()
        .ok_or("expected receipt object")?
        .keys()
    {
        if !exceptions.contains(&key.as_str()) {
            fields(current, archived, &[key])?;
        }
    }
    Ok(())
}

fn origin(current: &Value, archived: &Value) -> Result {
    fields(
        current,
        archived,
        &[
            "buildLocale",
            "cargoLockSha256",
            "compiler",
            "features",
            "profile",
            "revision",
            "sourceSha256",
            "target",
        ],
    )
}

fn check_original(current: &Value, archived: &Value, sealed: bool) -> Result {
    remaining(
        current,
        archived,
        &[
            "provenance",
            "holdout",
            "oracleSourceSha256",
            "runtime",
            "alternateZlib",
        ],
    )?;
    origin(&current["provenance"], &archived["provenance"])?;
    fields(
        &current["runtime"],
        &archived["runtime"],
        &["environment", "locale", "timezone"],
    )?;
    require(
        current["runtime"]["platform"] == "macos" && current["runtime"]["arch"] == "aarch64",
        "unexpected original host",
    )?;
    let alternate = &current["alternateZlib"];
    fields(
        alternate,
        &archived["alternateZlib"],
        &["independentZlibHex", "rawBytes", "rawSha256", "rustZlibHex"],
    )?;
    require(
        alternate["systemZlibVersion"] == archived["alternateZlib"]["pythonZlibVersion"]
            && alternate["directions"]
                == json!([
                    "Marmish encoder to system zlib decoder",
                    "system zlib encoder to Marmish decoder"
                ]),
        "alternate zlib implementation differs",
    )?;
    require(
        if sealed {
            current["holdout"] == archived["holdout"]
        } else {
            current.get("holdout") == Some(&Value::Null)
        },
        "sealed original receipt differs",
    )
}

fn check_data(
    current: &Value,
    archived: &Value,
    baseline: &Value,
    original: &Value,
    sealed: bool,
) -> Result {
    fields(
        current,
        baseline,
        &[
            "approvedGraphSha256",
            "publicCaseIds",
            "mixedPeers",
            "mutation",
            "comparedOperationResults",
        ],
    )?;
    remaining(
        current,
        archived,
        &[
            "build",
            "buildWithoutHints",
            "mutatedBuild",
            "originalCaller",
            "sealedCases",
            "driverSourceSha256",
            "runtime",
            "gaps",
        ],
    )?;
    origin(&current["originalCaller"], &archived["originalCaller"])?;
    for build in ["build", "buildWithoutHints", "mutatedBuild"] {
        fields(
            &current[build],
            &archived[build],
            &[
                "generatedSourceSha256",
                "graphFileSha256",
                "graphSha256",
                "projectionLockSha256",
            ],
        )?;
    }
    remaining(&current["runtime"], &archived["runtime"], &["host"])?;
    require(
        current["runtime"]["host"] == "macos",
        "unexpected Data host",
    )?;
    require(
        current["sourceCorpusCounts"]
            == json!({"prior":original["priorCases"],"authored":original["authoredCases"],"seeded":original["seededCases"]}),
        "Data corpus counts differ",
    )?;
    require(
        if sealed {
            current["sealedCases"] == archived["sealedCases"]
                && current["sealedCases"] == baseline["sealedCases"]
        } else {
            current["sealedCases"] == 0
        },
        "sealed Data count differs",
    )
}

fn main() -> Result {
    let args = Args::parse();
    let output = std::path::absolute(args.output)?;
    require(!output.try_exists()?, "output already exists")?;
    let upstream = args.upstream.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let tools = args.tooling_checkout.canonicalize()?;
    let sealed = args.holdout.map(fs::canonicalize).transpose()?;
    assembly::checkout(&upstream, assembly::ORIGINAL, false)?;
    assembly::checkout(&tools, TOOLS, true)?;
    let scratch = tempfile::tempdir()?;
    let compiler = scratch.path().join("oddlang");
    baseline_framework::build(&framework, &compiler)?;
    let dir = framework.join("examples/marmish/portable-oracle");
    let baseline = load(&dir.join("original-receipt.json"))?;
    let oracle_revision = text(&baseline["provenance"]["revision"])?;
    for path in ["Cargo.lock", "crates/marmish-stream"] {
        let object = |revision: &str| {
            stdout(
                Command::new("git")
                    .current_dir(&upstream)
                    .args(["rev-parse", &format!("{revision}:{path}")]),
            )
        };
        require(
            object(oracle_revision)? == object(assembly::ORIGINAL)?,
            format!("original corpus input changed: {path}"),
        )?;
    }
    let target = scratch.path().join("oracle-tools");
    run(assembly::build_env(&mut Command::new("cargo"))
        .current_dir(&tools)
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "--manifest-path",
            "verification/marmish-inventory/Cargo.toml",
            "--bin",
            "portable-oracle",
            "--bin",
            "qualify-tagged",
            "--target-dir",
        ])
        .arg(&target))?;
    let release = portable().join("copy-data-v1");
    let selection = dir.with_file_name("portable-contract");
    let recovered = scratch.path().join("data.ttl");
    run(Command::new(&compiler)
        .args(["recover", "tagged-contract"])
        .arg(&upstream)
        .arg(selection.join("selection.json"))
        .arg(selection.join("data-overlay.ttl"))
        .arg("urn:marmish:portable:copy-frame:v1:Data")
        .arg(&recovered))?;
    let published = load(&release.join("release.json"))?;
    for graph in [recovered, release.join("contract.ttl")] {
        require(
            stdout(
                Command::new(&compiler)
                    .args(["tagged-wire", "digest"])
                    .arg(graph),
            )?
            .trim()
                == text(&published["graphSha256"])?,
            "graph differs from published pin",
        )?;
    }
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    let parent = output.parent().ok_or("missing output parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    let archives = whole().join("stream");
    let native_pin = load(&archives.join("native-driver.json"))?;
    let execute = |tool: &str, profile: &str, file: &str| {
        let mut cmd = Command::new(target.join("debug").join(tool));
        cmd.arg("--upstream")
            .arg(&upstream)
            .args([
                "--original-target",
                host,
                "--original-revision",
                assembly::ORIGINAL,
                "--original-lock",
                LOCK,
                "--original-profile",
                profile,
                "--receipt",
            ])
            .arg(stage.path().join(file));
        if let Some(path) = &sealed {
            cmd.arg("--holdout").arg(path);
        }
        cmd
    };
    for profile in ["dev", "release"] {
        let name = format!("original-{profile}.json");
        let mut cmd = execute("portable-oracle", profile, &name);
        if profile == "dev" {
            cmd.arg("--observations")
                .arg(stage.path().join("public.json.gz"));
        }
        print!("{}", stdout(&mut cmd)?);
        let current = load(&stage.path().join(&name))?;
        fields(
            &current,
            &baseline,
            &[
                "publicObservationSha256",
                "priorCases",
                "authoredCases",
                "seededCases",
                "mixedBinaryAndJson",
                "negativeControls",
                "freezeSha256",
            ],
        )?;
        fields(&current, &native_pin, &["oracleSourceSha256"])?;
        if host == assembly::DARWIN {
            check_original(&current, &load(&archives.join(name))?, sealed.is_some())?;
        }
    }
    require(
        fs::read(stage.path().join("public.json.gz"))?
            == fs::read(archives.join("public.json.gz"))?,
        "compressed observations differ",
    )?;
    if host == assembly::DARWIN {
        let name = "data-release.json";
        print!(
            "{}",
            stdout(
                execute("qualify-tagged", "release", name)
                    .arg("--oddlang")
                    .arg(&compiler)
                    .arg("--graph")
                    .arg(release.join("contract.ttl"))
                    .arg("--hints")
                    .arg(release.join("hints.json"))
            )?
        );
        let current = load(&stage.path().join(name))?;
        fields(&current, &native_pin, &["driverSourceSha256", "gaps"])?;
        check_data(
            &current,
            &load(&archives.join(name))?,
            &load(&dir.join("tagged-qualification.json"))?,
            &baseline,
            sealed.is_some(),
        )?;
    }
    assembly::checkout(&tools, TOOLS, true)?;
    assembly::checkout(&upstream, assembly::ORIGINAL, false)?;
    let receipt = json!({"sourceRevision":baseline_framework::revision(),"freshBuildSha256":sha(&compiler)?,"cleanCheckout":true,"oracleToolsRevision":TOOLS,"host":host});
    fs::write(
        stage.path().join("framework.json"),
        serde_json::to_string_pretty(&receipt)? + "\n",
    )?;
    renameat_with(CWD, stage.path(), CWD, output, RenameFlags::NOREPLACE)?;
    Ok(())
}
