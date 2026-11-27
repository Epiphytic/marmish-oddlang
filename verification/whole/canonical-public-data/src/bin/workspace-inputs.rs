//! Prepare the pinned inputs and independent tests used by workspace qualification.
#[path = "../../../basic-command-interop/candidate_roles.rs"]
mod candidate_roles;
#[path = "../../../workspace/library_tests.rs"]
mod library_tests;
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, assembly};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Stage a workspace and its pinned package inputs for native qualification.
    Workspace {
        manifest: PathBuf,
        destination: PathBuf,
    },
    /// Check compiled executable names and report the CLI operations covered by each binding.
    CandidateRoles {
        inputs: PathBuf,
        binaries: Vec<String>,
    },
    /// Read Cargo JSON lines on stdin and verify the fresh release executable paths.
    ReleaseArtifacts { product: PathBuf },
    /// Check the sealed export against its independently published workspace lock.
    ExportProduct {
        product: PathBuf,
        pinned_lock: PathBuf,
    },
    /// Populate the offline Cargo Git cache from its locked local providers.
    WorkspaceProviders { lock: PathBuf, destination: PathBuf },
    /// List external package identities used to compare workspace and original locks.
    ExternalPackages { lock: PathBuf },
    /// Compare a generated workspace to the release receipt supplied on stdin.
    VerifyProduct {
        product: PathBuf,
        #[arg(long)]
        report: bool,
        #[arg(long)]
        unified: bool,
    },
    /// Attach original tests after checking the generated workspace.
    LibraryTests {
        product: PathBuf,
        original: PathBuf,
        stream_manifest: PathBuf,
        #[arg(long)]
        wire: bool,
        #[arg(long)]
        copy: bool,
    },
}
fn main() -> Result {
    let result = match Args::parse().action {
        Action::Workspace {
            manifest,
            destination,
        } => assembly::stage(&manifest, &destination)?,
        Action::CandidateRoles { inputs, binaries } => {
            candidate_roles::inspect(&inputs, std::io::stdin().lock(), binaries)?
        }
        Action::ReleaseArtifacts { product } => {
            candidate_roles::release_artifacts(&product, std::io::stdin().lock())?
        }
        Action::ExportProduct {
            product,
            pinned_lock,
        } => public_data_qualification::product::export_product(&product, &pinned_lock)?,
        Action::WorkspaceProviders { lock, destination } => {
            assembly::providers(&lock, &destination)?
        }
        Action::ExternalPackages { lock } => {
            serde_json::to_value(assembly::external_packages(&lock)?)?
        }
        Action::VerifyProduct {
            product,
            report,
            unified,
        } => {
            let lock = serde_json::from_reader(std::io::stdin().lock())?;
            public_data_qualification::product::verify_release_tree(
                &product, &lock, report, unified,
            )?;
            serde_json::Value::Null
        }
        Action::LibraryTests {
            product,
            original,
            stream_manifest,
            wire,
            copy,
        } => library_tests::stage(&product, &original, &stream_manifest, wire, copy)?,
    };
    println!("{result}");
    Ok(())
}
