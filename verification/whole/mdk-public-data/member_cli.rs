//! Entry points for the integrated workspace witness using its selected build artifacts.
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, mdk};
use serde_json::json;
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    step: Step,
}
#[derive(Subcommand)]
enum Step {
    Recover {
        oddlang: PathBuf,
        source: PathBuf,
        work: PathBuf,
        inputs: PathBuf,
    },
    Observe {
        library: PathBuf,
        deps: PathBuf,
        serde: PathBuf,
        serde_deps: PathBuf,
        work: PathBuf,
    },
}
fn main() -> Result {
    let result = match Args::parse().step {
        Step::Recover {
            oddlang,
            source,
            work,
            inputs,
        } => mdk::recover(&oddlang, &source, &work, &inputs)?,
        Step::Observe {
            library,
            deps,
            serde,
            serde_deps,
            work,
        } => {
            let (out, negative) = mdk::observe(&library, &deps, &serde, &serde_deps, &work)?;
            json!({"output":String::from_utf8(out)?,"negativeRustDiagnostics":negative})
        }
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
