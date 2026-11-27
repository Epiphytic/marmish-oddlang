#[path = "../offline-cli/observe.rs"]
mod offline_cli;
#[path = "../public-diagnostics/verify.rs"]
mod diagnostics;
#[path = "../private-buffer-inference/source.rs"]
mod buffer_trace;
#[path = "../gap-range-endpoint/contract.rs"]
mod endpoint_contract;
#[path = "../suite/controls.rs"]
mod suite_controls;
#[path = "../suite/selected.rs"]
mod suite_selected;
use public_data_qualification::suite_stage;
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
    ImportManifest { product: PathBuf, inputs: PathBuf, release: String, revision: String, digest: String },
    ImportArtifacts { package_root: PathBuf, target: PathBuf, #[arg(required = true)] packages: Vec<String> },
    PublicImports { original: PathBuf, host: String },
    ImportControl { work: PathBuf, host: String },
    PrototypeSurface { host: String },
    OfflineCliCase {
        binary: PathBuf,
        home: PathBuf,
        #[arg(last = true)]
        argv: Vec<String>,
    },
    CompareOfflineCli {
        original: PathBuf,
        baseline: PathBuf,
        output: PathBuf,
        baseline_build: PathBuf,
        successor: PathBuf,
        successor_build: PathBuf,
    },
    ComposeCli {
        #[arg(value_enum)]
        release: public_data_qualification::cli_composition::Release,
        compiler: PathBuf,
        original: PathBuf,
        portable: PathBuf,
        #[arg(long)]
        write: bool,
    },
    DiagnosticCaller {
        library: PathBuf,
        host: String,
        version: String,
        work: PathBuf,
        host_deps: Option<PathBuf>,
    },
    PublicDiagnostics {
        original: PathBuf,
        target: PathBuf,
        receipt: PathBuf,
    },
    EngineCaller {
        #[arg(value_enum)]
        role: public_data_qualification::engine_caller::Role,
        library: PathBuf,
        deps: PathBuf,
        serde: PathBuf,
        work: PathBuf,
    },
    RuntimeStatusCaller {
        work: PathBuf,
        #[arg(long)]
        native: Option<PathBuf>,
    },
    RuntimeStatusInputs {
        compiler: PathBuf,
        original: PathBuf,
        destination: PathBuf,
    },
    EndpointContract,
    EndpointProfiles {
        work: PathBuf,
    },
    EndpointInputs {
        work: PathBuf,
        profile: String,
    },
    MdkHost {
        binary: PathBuf,
    },
    BufferTraces {
        original: PathBuf,
        target: PathBuf,
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
    BufferTraceCase {
        checkout: PathBuf,
        target: PathBuf,
        host: String,
        name: String,
    },
    BufferTraceMutation {
        original: PathBuf,
        checkout: PathBuf,
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
    CanonicalOfflineProduct {
        product: PathBuf,
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
    SuiteReaderStage {
        original: PathBuf,
        prototype: PathBuf,
    },
    SuiteMdkCase {
        original: PathBuf,
        product: Option<PathBuf>,
    },
    SuiteSelectedCases {
        product: PathBuf,
        host: String,
        suite: String,
    },
    SuiteParserCases {
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
        #[arg(long)]
        inherited_work: Option<PathBuf>,
        #[arg(long, conflicts_with="inherited_work")]
        records_work: Option<PathBuf>,
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
        Action::ImportManifest { product, inputs, release, revision, digest } => {
            product::public_product_at(&product, &inputs, &release, &revision, &digest)?.1
        }
        Action::ImportArtifacts { package_root, target, packages } => {
            let names: Vec<_> = packages.iter().map(String::as_str).collect();
            let log: String = serde_json::from_reader(io::stdin().lock())?;
            let artifacts = public_data_qualification::artifacts::selected_build(
                log.as_bytes(), &package_root.canonicalize()?, &target.canonicalize()?, &names,
            )?;
            serde_json::to_value(artifacts)?
        }
        Action::PublicImports { original, host } => imports::library_set(&host, &original, &input()?)?,
        Action::ImportControl { work, host } => {
            let data = input()?;
            imports::probe(&host, &data["role"], &work, public_data_qualification::text(&data["body"])?)?
        }
        Action::PrototypeSurface { host } => {
            let data = input()?;
            imports::prototype(&host, &data["roles"], &data["links"])?
        }
        Action::RuntimeStatusCaller { work, native } => {
            domains::runtime_from_cargo(io::stdin().lock(), &work, native.as_deref())?
        }
        Action::RuntimeStatusInputs {
            compiler,
            original,
            destination,
        } => {
            public_data_qualification::runtime_status_recovery::prepare(
                &compiler,
                &original,
                &destination,
            )?;
            Value::Null
        }
        Action::OfflineCliCase {binary, home, argv} => offline_cli::observe(&binary, &home, &argv)?,
        Action::CompareOfflineCli { original, baseline, output, baseline_build, successor, successor_build } => {
            offline_cli::compare(&original, &baseline, &baseline_build, &successor, &successor_build, &output)?;
            return Ok(());
        }
        Action::ComposeCli { release, compiler, original, portable, write } => {
            public_data_qualification::cli_composition::compose(release, &portable, &compiler, &original, write)?;
            return Ok(());
        }
        Action::DiagnosticCaller { library, host, version, work, host_deps } => {
            diagnostics::observe(&library, &host, &version, &work, host_deps.as_deref())?
        }
        Action::PublicDiagnostics { original, target, receipt } => {
            diagnostics::baseline(&original, &target, &receipt)?;
            return Ok(());
        }
        Action::EngineCaller {
            role,
            library,
            deps,
            serde,
            work,
        } => serde_json::to_value(public_data_qualification::engine_caller::observe(
            role, &library, &deps, &serde, &work,
        )?)?,
        Action::BufferTraces {
            original,
            target,
            receipt,
        } => {
            let report =
                buffer_trace::observe(&original.canonicalize()?, &std::path::absolute(target)?)?;
            if let Some(receipt) = receipt {
                std::fs::write(receipt, public_data_qualification::json_bytes(&report)?)?;
                return Ok(());
            }
            report
        }
        Action::BufferTraceCase {
            checkout,
            target,
            host,
            name,
        } => buffer_trace::capture(&checkout, &std::path::absolute(target)?, &host, &name)?,
        Action::BufferTraceMutation { original, checkout } => {
            buffer_trace::prepare(&original.canonicalize()?, &checkout)?
        }
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
        Action::CanonicalOfflineProduct { product } => public_data_qualification::check_canonical_offline(&product)?,
        Action::ManifestProduct { product } => {
            public_data_qualification::manifest_product::check(&product)?
        }
        Action::EndpointProfiles { work } => endpoint_contract::profiles(&work)?,
        Action::EndpointContract => endpoint_contract::validate()?,
        Action::EndpointInputs { work, profile } => endpoint_contract::stage(&work, &profile)?,
        Action::ReaderTest { original } => {
            serde_json::json!({"module":buffered::original_test(&buffered::reader_source(&original)?)?,"bodySha256":buffered::READER_TEST_SHA})
        }
        Action::SuiteVaultEngine { original, product } => {
            suite_stage::vault_engine(&original, &product)?
        }
        Action::SuiteReaderStage {
            original,
            prototype,
        } => suite_stage::reader(&original, &prototype)?,
        Action::SuiteMdkCase { original, product } => {
            suite_stage::mdk(&original, product.as_deref())?
        }
        Action::SuiteSelectedCases {
            product,
            host,
            suite,
        } => suite_selected::exercise(&product, &host, &suite)?,
        Action::SuiteParserCases { product } => suite_controls::parser_cases(&product)?,
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
            inherited_work, records_work,
        } => {
            use public_data_qualification::manifest_native::{ParserRun, rows};
            let build = rows(&std::fs::read(artifacts)?)?;
            let parser = ParserRun {
                product: &product,
                original: &original,
                original_target: &original_target,
                scratch: &scratch,
                host: &host,
                build: &build,
            };
            if let Some(work) = records_work {
                serde_json::json!({"publicRecords":parser.records(&work,&work.join("oddlang"))?})
            } else if let Some(work) = inherited_work {
                parser.inherited(&work,&work.join("oddlang"))?
            } else {
                parser.observe(None)?
            }
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
