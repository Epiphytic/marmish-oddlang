//! Compare local status reply decoding with observations from a pinned original client.
use clap::{ArgGroup, Parser, ValueEnum};
use public_data_qualification::{
    Result, assembly::bytes_sha, json_bytes, load, require, sha, status_peer::replay, whole,
};
use serde_json::json;
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, ValueEnum)]
enum Corpus {
    Extended,
    Nested,
}
#[derive(Parser)]
#[command(group(ArgGroup::new("reference").required(true).args(["compare","reference_status"])))]
struct Args {
    corpus: Corpus,
    binary: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    reference_status: Option<PathBuf>,
    #[arg(long)]
    compare: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let binary = args.binary.canonicalize()?;
    let digest = sha(&binary)?;
    let file = match args.corpus {
        Corpus::Extended => "reply-shapes.json",
        Corpus::Nested => "typed-reply-variants.json",
    };
    let bytes = fs::read(whole().join("status").join(file))?;
    let hash = bytes_sha(&bytes);
    let candidate = args.compare.is_some();
    let reference = load(
        args.compare
            .as_ref()
            .or(args.reference_status.as_ref())
            .ok_or("missing reference")?,
    )?;
    require(
        reference["mode"] == "original",
        "expected original reference",
    )?;
    if candidate {
        require(reference["caseSha256"] == hash, "corpus digest differs")?;
    } else {
        require(
            reference["originalBinaries"]["marmish"] == digest,
            "original binary digest differs",
        )?;
    }
    let results = replay(&binary, &serde_json::from_slice(&bytes)?)?;
    let rows = results.as_object().ok_or("missing results")?;
    let mut receipt = json!({"binarySha256":digest,"caseSha256":hash,"mode":if candidate{"candidate"}else{"original"},"originalRevision":reference.get("originalRevision").ok_or("missing original revision")?,"results":results});
    let mut mismatches = Vec::new();
    if candidate {
        let expected = reference["results"]
            .as_object()
            .ok_or("missing reference results")?;
        require(
            expected.len() == rows.len() && rows.keys().all(|k| expected.contains_key(k)),
            "case names differ",
        )?;
        mismatches.extend(
            rows.iter()
                .filter(|(k, v)| expected[*k] != **v)
                .map(|(k, _)| k.as_str()),
        );
        mismatches.sort();
        receipt["mismatches"] = json!(mismatches);
    }
    receipt.sort_all_objects();
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    require(
        mismatches.is_empty(),
        format!("different replies: {}", mismatches.join(", ")),
    )?;
    if matches!(args.corpus, Corpus::Nested) {
        println!("Observed {} typed local reply variants", rows.len());
    }
    Ok(())
}
