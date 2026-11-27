use crate::{
    Cases, corpus,
    manifest::{MAX, Record, fault},
};
use public_data_qualification::assembly::bytes_sha;
use serde_json::json;
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
