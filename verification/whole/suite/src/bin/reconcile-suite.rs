//! Reconcile a named suite inventory with test names observed by the producer.
use serde_json::Value;
use std::{collections::BTreeMap, io};
use suite_receipts::{Result, reconcile};

fn main() -> Result<()> {
    let (inventory, passed): (Value, BTreeMap<String, Vec<String>>) =
        serde_json::from_reader(io::stdin().lock())?;
    serde_json::to_writer(io::stdout().lock(), &reconcile(&inventory, passed)?)?;
    Ok(())
}
