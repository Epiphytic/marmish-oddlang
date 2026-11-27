#[cfg(test)]
use std::io::Write;
mod corpus;
mod oracle;
use clap::Parser;
use copy_checkpoint_replay::{build_compiler, compare, consumer, stage};
use oracle::{DOMAIN, decode, fault};
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox, bytes_sha},
    json_bytes, load, portable, require, run, sha, stdout, text, whole,
};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};
const FRAMEWORK: &str = "3b3a1ef0e560041474cf6cddedcd52ab66defde2";
const PACKAGE: &str = "stream-copy-checkpoint-package.json";
#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}
fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    let framework = a.framework.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    assembly::checkout(&framework, FRAMEWORK, false)?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let private = tempfile::tempdir()?;
    let cli = build_compiler(&framework, &private.path().join("tools"), "+1.97.1")?;
    let compiler = work.join("oddlang");
    fs::copy(cli, &compiler)?;
    let (plan, mut manifest) = stage(&work.join("inputs"), PACKAGE, false)?;
    let caller = whole().join("copy-checkpoint/caller.rs");
    let root = assembly::root().canonicalize()?;
    let cache = work.join("empty-provider-cache");
    fs::create_dir(&cache)?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("host missing")?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&root, &original, &framework, private.path()],
        &cache,
    )?;
    let project = |name: &str| -> Result<PathBuf> {
        let dir = work.join(name);
        sandbox.deny(&[caller.clone(), original.join("Cargo.toml")])?;
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(&plan)
            .arg(&dir))?;
        run(sandbox
            .command(&dir)?
            .args(["cargo", "build", "--locked", "--offline", "--release"]))?;
        Ok(dir)
    };
    let product = project("native")?;
    let lock = product.join("package.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join("copy-checkpoint-v1/package.lock"))?,
        "checkpoint package lock differs",
    )?;
    let old = consumer(
        &original.join("crates/marmish-stream"),
        &private.path().join("original"),
        &original.join("Cargo.lock"),
        &caller,
    )?;
    let new = consumer(
        &product,
        &private.path().join("generated"),
        &product.join("Cargo.lock"),
        &caller,
    )?;
    let (records, wires) = corpus::cases();
    let mut requests = Vec::new();
    let mut expected = Vec::new();
    for r in &records {
        requests.push(json!({"operation":"construct","record":r}));
        let result = match r.validate() {
            Ok(()) => json!({"validation":{"ok":null},"encoding":{"ok":r.encode(DOMAIN)}}),
            Err(e) => json!({"validation":fault(e),"encoding":fault(e)}),
        };
        expected.push(result);
    }
    let mut rejected = 0;
    for w in &wires {
        requests.push(json!({"operation":"decode","bytes":w}));
        expected.push(match decode(w) {
            Ok(record) => json!({"ok":record}),
            Err(e) => {
                rejected += 1;
                fault(e)
            }
        });
    }
    compare("original", &old, &requests, &expected)?;
    compare("generated", &new, &requests, &expected)?;
    let pin = &mut manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?
        .last_mut()
        .ok_or("empty components")?["graph"];
    let graph_path = plan.parent().unwrap().join(text(&pin["path"])?);
    let mut graph = load(&graph_path)?;
    let mut alternate: Vec<u8> = serde_json::from_value(graph["integrity"]["domain"].clone())?;
    require(alternate == DOMAIN, "unexpected checksum domain")?;
    alternate[0] ^= 1;
    graph["integrity"]["domain"] = json!(alternate);
    fs::write(&graph_path, json_bytes(&graph)?)?;
    pin["sha256"] = json!(sha(&graph_path)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let mutant = project("mutant")?;
    let observer = consumer(
        &mutant,
        &private.path().join("mutation"),
        &mutant.join("Cargo.lock"),
        &caller,
    )?;
    compare(
        "changed integrity domain",
        &observer,
        &[
            requests[0].clone(),
            json!({"operation":"decode","bytes":records[0].encode(DOMAIN)}),
        ],
        &[
            json!({"validation":{"ok":null},"encoding":{"ok":records[0].encode(&alternate)}}),
            fault("copy_checkpoint_checksum"),
        ],
    )?;
    let receipt = json!({"original":ORIGINAL,"framework":FRAMEWORK,"rust":rust.trim(),"originalAndCompanionReadDenied":true,"packageLockSha256":sha(&lock)?,"callerSha256":sha(&caller)?,"constructions":records.len(),"decodes":wires.len(),"rejectedDecodes":rejected,"observationsSha256":bytes_sha(&serde_json::to_vec(&expected)?),"negativeControl":"repinning the integrity domain changes the encoded footer and rejects the original digest"});
    fs::write(a.receipt, json_bytes(&receipt)?)?;
    println!(
        "Qualified {} native Checkpoint constructions and {} binary decodes.",
        records.len(),
        wires.len()
    );
    Ok(())
}
#[test]
fn request_order_and_bytes_match_python() -> Result {
    let (r, w) = corpus::cases();
    let mut bytes = Vec::new();
    for row in r
        .iter()
        .map(|v| json!({"operation":"construct","record":v}))
        .chain(w.iter().map(|v| json!({"operation":"decode","bytes":v})))
    {
        serde_json::to_writer(&mut bytes, &row)?;
        writeln!(bytes)?;
    }
    assert_eq!(
        bytes_sha(&bytes),
        "91f711a30909d0b0bd3ec5d4379deb808c984887ecaa81291bb515faad4d1c84"
    );
    Ok(())
}
