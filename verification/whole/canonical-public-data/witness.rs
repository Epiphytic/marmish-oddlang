use clap::{Parser, Subcommand};
use public_data_qualification::{Result, buffered, domains, imports, product};
use serde_json::Value;
use std::{io, path::PathBuf};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
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
    Domains,
}
fn main() -> Result {
    let input = || serde_json::from_reader(io::stdin().lock());
    let report = match Args::parse().action {
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
        Action::Domains => domains::observe(&input()?)?,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
