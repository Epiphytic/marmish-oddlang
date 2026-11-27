//! Build the graph-only client in the macOS generation sandbox, then run its independent oracle.
use clap::{Args, Parser, Subcommand};
use public_data_qualification::{Result, require, stdout, whole};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    suite: Suite,
}

#[derive(Subcommand)]
enum Suite {
    Parser {
        framework: PathBuf,
        original: PathBuf,
        output: PathBuf,
    },
    Status(StatusArgs),
    TypedStatus(StatusArgs),
    Discovery(StatusArgs),
}

#[derive(Args)]
struct StatusArgs {
    framework: PathBuf,
    original: PathBuf,
    original_bins: PathBuf,
    output: PathBuf,
    receipt: PathBuf,
}

fn checked(cmd: &mut Command) -> Result<()> {
    let status = cmd.status()?;
    require(status.success(), format!("{cmd:?} failed ({status})"))
}

fn generate(
    framework: &Path,
    original: &Path,
    output: &Path,
    pin: &str,
    project: &str,
    fixture: &str,
    binaries: Option<&Path>,
) -> Result<()> {
    let mut build = Command::new("bash");
    build
        .arg(whole().join("native-build/macos.sh"))
        .arg(framework)
        .arg(original)
        .arg(output)
        .arg(pin)
        .arg(whole().join("canonical-public-data/src/bin").join(project))
        .arg(whole().join(fixture));
    if let Some(bins) = binaries {
        build.arg(bins);
    }
    checked(&mut build)
}

fn main() -> Result<()> {
    let suite = Cli::parse().suite;
    require(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "qualification requires macOS ARM64",
    )?;
    let (args, pin, project, oracle) = match suite {
        Suite::Parser {
            framework,
            original,
            output,
        } => {
            generate(
                &framework,
                &original,
                &output,
                "c01279eb61ed9e86b51b2eeaf0c5b744397c74a3",
                "cli-project.rs",
                "cli/transcript.json",
                None,
            )?;
            return checked(
                Command::new("cargo")
                    .args([
                        "+1.96.1",
                        "run",
                        "--quiet",
                        "--locked",
                        "--offline",
                        "--manifest-path",
                    ])
                    .arg(whole().join("canonical-public-data/Cargo.toml"))
                    .args(["--bin", "cli-transcript", "--", "verify"])
                    .arg(output.join("target/release")),
            );
        }
        Suite::Status(a) => (
            a,
            "c94b678a9672a75df9fe761dad62cfba9aaff33f",
            "status-project.rs",
            "qualify.py",
        ),
        Suite::TypedStatus(a) => (
            a,
            "ee4ba72af452aecb0d50c193c6b642552f8f0a79",
            "typed-status-project.rs",
            "typed_qualify.py",
        ),
        Suite::Discovery(a) => (
            a,
            "a882f1cfe4a65880cb038f3ea32ccd7e7b271591",
            "discovery-status-project.rs",
            "discovery_probe.py",
        ),
    };
    require(!args.receipt.try_exists()?, "receipt already exists")?;
    require(
        stdout(Command::new("git").current_dir(whole()).args([
            "status",
            "--porcelain",
            "--untracked-files=all",
        ]))?
        .is_empty(),
        "expected a clean companion checkout",
    )?;
    let parent = args
        .receipt
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = tempfile::tempdir_in(parent)?;
    let report = tmp.path().join("receipt.json");
    let release = std::path::absolute(&args.output)?.join("target/release");
    generate(
        &args.framework,
        &args.original,
        &args.output,
        pin,
        project,
        "status/macos-arm64.json",
        Some(&args.original_bins),
    )?;
    let mut probe = Command::new("python3");
    probe
        .arg("-B")
        .arg(whole().join("status").join(oracle))
        .arg(args.original_bins.canonicalize()?);
    if oracle == "discovery_probe.py" {
        probe
            .arg(&report)
            .arg("--candidate")
            .arg(release.join("marmish"));
    } else {
        probe.arg(release).arg(&report);
    }
    checked(probe.args(["--framework-revision", pin]))?;
    // Publish only a finished report, keeping a concurrently created receipt intact.
    std::fs::hard_link(report, args.receipt)?;
    Ok(())
}
