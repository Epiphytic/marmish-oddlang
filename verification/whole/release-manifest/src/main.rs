//! Export independent release manifest cases, or compare a consumer's ordered output.
mod corpus;
mod strict;
use corpus::Case;
use serde::Serialize;
use serde_json::{
    Value, json,
    ser::{Formatter, Serializer},
};
use std::{
    env,
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct PortableFormat;
impl Formatter for PortableFormat {
    fn begin_array_value<W: Write + ?Sized>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if first { Ok(()) } else { w.write_all(b", ") }
    }
    fn begin_object_key<W: Write + ?Sized>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if first { Ok(()) } else { w.write_all(b", ") }
    }
    fn begin_object_value<W: Write + ?Sized>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(b": ")
    }
}
fn vectors(cases: &[Case], inputs: bool) -> Result<Vec<u8>> {
    let mut out = b"[\n".to_vec();
    for (i, c) in cases.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b",\n");
        }
        let mut row = json!({"case":c.name,"hex":hex::encode(&c.raw)});
        if !inputs {
            row["result"] = c.result.clone();
        }
        row.serialize(&mut Serializer::with_formatter(&mut out, PortableFormat))?;
    }
    out.extend_from_slice(b"\n]\n");
    Ok(out)
}
fn check(rows: &Value, cases: &[Case]) -> Result<()> {
    let rows = rows
        .as_array()
        .filter(|r| r.len() == cases.len())
        .ok_or_else(|| format!("expected {} ordered result rows", cases.len()))?;
    for (row, c) in rows.iter().zip(cases) {
        if *row != json!({"case":c.name,"result":c.result}) {
            return Err(format!("result mismatch at {}", c.name).into());
        }
    }
    Ok(())
}
fn archives(root: &Path) -> Result<()> {
    let cases = corpus::cases(false);
    if vectors(&cases, false)? != fs::read(root.join("vectors.json"))? {
        return Err("archived vectors differ".into());
    }
    let mut previous = None;
    for host in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let strict::Strict(mut receipt) =
            serde_json::from_slice(&fs::read(root.join(format!("{host}.json")))?)?;
        let compiler = receipt
            .as_object_mut()
            .ok_or("missing receipt")?
            .remove("compiler")
            .ok_or("missing compiler")?;
        if !compiler
            .as_array()
            .ok_or("invalid compiler")?
            .contains(&json!(format!("host: {host}")))
        {
            return Err("wrong recorded host".into());
        }
        check(&receipt["baseline"], &cases)?;
        if receipt["changedCases"] != json!(["size-3-16777217"]) {
            return Err("wrong mutation".into());
        }
        let mut changed = receipt["baseline"].clone();
        let row = changed
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["case"] == "size-3-16777217")
            .unwrap();
        *row = receipt["mutation"][0].clone();
        check(&changed, &corpus::cases(true))?;
        if previous.as_ref().is_some_and(|r| r != &receipt) {
            return Err("host receipts differ".into());
        }
        previous = Some(receipt);
    }
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [mode, path] = args.as_slice() else {
        return Err("expected inputs, vectors, changed-vectors, check, check-changed or archives and a path".into());
    };
    let mode = mode.to_str().ok_or("invalid mode")?;
    let path = Path::new(path);
    let cases = corpus::cases(matches!(mode, "check-changed" | "changed-vectors"));
    match mode {
        "inputs" | "vectors" | "changed-vectors" => {
            fs::write(path, vectors(&cases, mode == "inputs")?)?
        }
        "check" | "check-changed" | "check-receipt" => {
            let strict::Strict(rows) = serde_json::from_slice(&fs::read(path)?)?;
            check(
                if mode == "check-receipt" {
                    &rows["baseline"]
                } else {
                    &rows
                },
                &cases,
            )?;
            println!("{} manifest results match", cases.len());
        }
        "archives" => archives(path)?,
        _ => return Err("unknown release manifest conformance command".into()),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_vectors_and_both_native_oracles() {
        archives(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    }
    #[test]
    fn malformed_observations() {
        let c = Case {
            name: "sample".into(),
            raw: vec![b'x'],
            result: json!({"count":1,"enabled":false}),
        };
        for rows in [
            json!([]),
            json!([{"case":"sample","result":{"count":true,"enabled":0}}]),
            json!([{"case":"other","result":c.result}]),
            json!([{"case":"sample","result":c.result,"extra":1}]),
        ] {
            assert!(check(&rows, std::slice::from_ref(&c)).is_err());
        }
        assert!(
            serde_json::from_str::<strict::Strict>(r#"{"nested":{"id":1,"id":1}}"#)
                .err()
                .unwrap()
                .to_string()
                .contains("duplicate JSON member")
        );
    }
}
