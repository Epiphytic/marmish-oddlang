//! Compare an inspected native suite receipt with upstream Cargo metadata.
use std::{env, path::Path};
use suite_receipts::{Result, candidate_targets, read};
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [file] = args.as_slice() else {
        return Err("usage: candidate-targets RECEIPT".into());
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&candidate_targets(&read(Path::new(file))?)?)?
    );
    Ok(())
}
