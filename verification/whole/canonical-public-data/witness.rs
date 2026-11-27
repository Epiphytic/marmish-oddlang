#[path = "../suite/controls.rs"]
mod suite_controls;
#[path = "../suite/stage.rs"]
mod suite_stage;
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, api_composition, buffered, domains, imports, product};
use serde_json::Value;
use std::{io, path::PathBuf};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    MdkHost {
        binary: PathBuf,
    },
    ChildControl {
        original: PathBuf,
        compiler: PathBuf,
    },
    CountExecution {
        name: String,
        runs: usize,
    },
    Members {
        product: PathBuf,
    },
    Six {
        product: PathBuf,
    },
    Canonical {
        baseline: PathBuf,
        product: PathBuf,
        revision: String,
    },
    Imports {
        host: String,
        original: PathBuf,
        work: PathBuf,
    },
    BufferedProduct {
        product: PathBuf,
    },
    BufferedOracle {
        product: PathBuf,
        original: PathBuf,
        host: String,
        #[arg(long)]
        prior: Option<PathBuf>,
    },
    ComposeApi {
        #[arg(value_enum)]
        release: api_composition::Release,
        portable: PathBuf,
        #[arg(long)]
        check: bool,
    },
    ManifestProduct {
        product: PathBuf,
    },
    ReaderTest {
        original: PathBuf,
    },
    SuiteVaultEngine {
        original: PathBuf,
        product: PathBuf,
    },
    SuiteParser {
        original: PathBuf,
        product: PathBuf,
    },
    ManifestSelection {
        original: PathBuf,
        native: PathBuf,
    },
    ManifestOracle {
        product: PathBuf,
        original: PathBuf,
        original_target: PathBuf,
        scratch: PathBuf,
        host: String,
        artifacts: PathBuf,
    },
    RangeTree {
        root: PathBuf,
    },
    RangeHash,
    RangeLockDelta {
        #[arg(default_value = "")]
        path: String,
    },
    RangeClosure {
        target: PathBuf,
        host: String,
    },
    Domains,
}
fn main() -> Result {
    let input = || serde_json::from_reader(io::stdin().lock());
    let report = match Args::parse().action {
        Action::MdkHost { binary } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&suite_controls::mdk(&binary.canonicalize()?)?)?
            );
            return Ok(());
        }
        Action::ChildControl { original, compiler } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&suite_controls::child(
                    &original.canonicalize()?,
                    &compiler.canonicalize()?
                )?)?
            );
            return Ok(());
        }
        Action::CountExecution { name, runs } => {
            let (raw, passing): (String, Vec<String>) =
                serde_json::from_reader(io::stdin().lock())?;
            let names = passing.iter().map(String::as_str).collect::<Vec<_>>();
            Value::Bool(suite_controls::counted(&name, runs, &raw, &names))
        }
        Action::Members { product: p } => {
            product::check_members(&p, &input()?, false)?;
            Value::Null
        }
        Action::Six { product: p } => product::six(&p)?,
        Action::Canonical {
            baseline,
            product: p,
            revision,
        } => product::snapshots(&baseline, &p, &revision)?,
        Action::Imports {
            host,
            original,
            work,
        } => imports::observe(&host, &original, &work, &input()?)?,
        Action::BufferedProduct { product } => buffered::check(&product)?,
        Action::BufferedOracle {
            product,
            original,
            host,
            prior,
        } => buffered::observe(&product, &original, &host, prior.as_deref())?,
        Action::ComposeApi {
            release,
            portable,
            check,
        } => {
            api_composition::compose(release, &portable, check)?;
            if matches!(release, api_composition::Release::EnginePublicError) {
                println!("{}", portable.join(api_composition::ERRORS).display());
            }
            return Ok(());
        }
        Action::ManifestProduct { product } => {
            public_data_qualification::manifest_product::check(&product)?
        }
        Action::ReaderTest { original } => {
            serde_json::json!({"module":buffered::original_test(&buffered::reader_source(&original)?)?,"bodySha256":buffered::READER_TEST_SHA})
        }
        Action::SuiteVaultEngine { original, product } => {
            suite_stage::vault_engine(&original, &product)?
        }
        Action::SuiteParser { original, product } => suite_stage::parser(&original, &product)?,
        Action::ManifestSelection { original, native } => {
            use public_data_qualification::manifest_product::*;
            let (leaf, prefix) = leaf(&native)?;
            serde_json::json!({"tests":String::from_utf8(tests(&original)?)?,"leaf":leaf,"prefix":prefix,"vector":VECTOR})
        }
        Action::ManifestOracle {
            product,
            original,
            original_target,
            scratch,
            host,
            artifacts,
        } => {
            use public_data_qualification::manifest_native::{ParserRun, rows};
            let build = rows(&std::fs::read(artifacts)?)?;
            ParserRun {
                product: &product,
                original: &original,
                original_target: &original_target,
                scratch: &scratch,
                host: &host,
                build: &build,
            }
            .observe(None)?
        }
        Action::RangeTree { root } => public_data_qualification::range::tree(&root)?,
        Action::RangeHash => public_data_qualification::range::hashed(&input()?)?.into(),
        Action::RangeLockDelta { path } => {
            let input: Value = input()?;
            public_data_qualification::range::delta(&input["old"], &input["new"], &path)
        }
        Action::RangeClosure { target, host } => {
            let raw: String = serde_json::from_reader(io::stdin().lock())?;
            public_data_qualification::range::library_closure(raw.as_bytes(), &target, &host)?
        }
        Action::Domains => domains::observe(&input()?)?,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
