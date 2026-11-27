use clap::Parser;
use copy_manifest_replay::{
    Cases, corpus,
    manifest::{MAX, Record, fault},
    verify,
};
use public_data_qualification::{Result, assembly::bytes_sha};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    receipt: PathBuf,
}
pub fn cases() -> Cases {
    let (records, wires) = corpus::cases();
    let mut requests = vec![];
    let mut expected = vec![];
    let mut hashes = vec![];
    for r in &records {
        requests.push(json!({"operation":"construct","manifest":r}));
        let (v, e) = match r.validate() {
            Ok(()) => {
                let b = r.encode();
                hashes.push(bytes_sha(&b));
                (json!({"ok":null}), json!({"ok":b}))
            }
            Err(e) => (fault(e), fault(e)),
        };
        expected.push(json!({"validation":v,"encode":e,"namespace":r.namespace(false)}))
    }
    let mut rejected = 0;
    for w in &wires {
        requests.push(json!({"operation":"decode","bytes":w}));
        expected.push(match Record::decode(w) {
            Ok(r) => json!({"ok":{"manifest":r,"namespace":r.namespace(false)}}),
            Err(e) => {
                rejected += 1;
                fault(e)
            }
        })
    }
    let mut changed = expected[0].clone();
    changed["namespace"] = json!(records[0].namespace(true));
    assert_ne!(changed, expected[0]);
    let mut bad = records[0].clone();
    bad.size = MAX + 1;
    let wire = bad.encode();
    let prefix = &wire[..101 + 8 + bad.source.len() + bad.destination.len() + 3 + 8];
    assert_eq!(Record::decode(prefix).err(), Some("Offset"));
    let mutant = vec![
        (requests[0].clone(), changed),
        (
            json!({"operation":"decode","bytes":prefix}),
            fault("copy_truncated"),
        ),
    ];
    Cases {
        requests,
        expected,
        mutant,
        metadata: json!({"constructions":records.len(),"decodes":wires.len(),"rejectedDecodes":rejected,"encodedDigests":hashes.len(),"digestListSha256":bytes_sha(hashes.concat().as_bytes())}),
        negative: "repinned namespace order changes exactly the predicted digest; deferring the size range makes truncation win over Offset",
    }
}
fn mutate(g: &mut Value) -> Result {
    let view = g["hashViews"]
        .as_object_mut()
        .and_then(|v| v.values_mut().next())
        .ok_or("no hash view")?;
    view["fields"]
        .as_array_mut()
        .ok_or("no namespace fields")?
        .swap(0, 1);
    let mut slots = g["layout"]
        .as_array_mut()
        .ok_or("no layout")?
        .iter_mut()
        .filter(|s| s["id"].as_str().is_some_and(|s| s.ends_with(":field:size")));
    let slot = slots.next().ok_or("size absent")?;
    assert!(slots.next().is_none());
    slot["encoding"]
        .as_object_mut()
        .ok_or("no encoding")?
        .remove("readError")
        .ok_or("no readError")?;
    Ok(())
}
fn main() -> Result {
    let a = Args::parse();
    verify(
        "copy-manifest",
        "e29873828a015a1af201e2887cae85ff4b2faeb8",
        &a.original,
        &a.framework,
        &a.receipt,
        cases(),
        mutate,
    )
}
#[test]
fn archived_observations() {
    let c = cases();
    let mut rows = json!(c.requests);
    rows.sort_all_objects();
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&rows).unwrap()),
        "13c337b7afd861651671b0bbecf6ece334fff585d66a7b1f1a85a289c53258b0"
    );
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&c.expected).unwrap()),
        "ef90454f108ea6d946a00008f73d37116d491710463ac806eee94792e3f57cc8"
    );
}
