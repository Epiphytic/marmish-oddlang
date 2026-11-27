//! Independent constructions, ordered truncations, resealed inputs and all wire tags.
use crate::oracle::{DOMAIN, LIMIT, PHASES, Record, seal, sum};
use std::collections::BTreeSet;
fn nested(src: &[u8], dst: &[u8], size: u64, hash: [u8; 32]) -> Vec<u8> {
    let mut b = 1u32.to_be_bytes().to_vec();
    b.extend(0u8..96);
    b.push(1);
    for path in [src, dst] {
        b.extend((path.len() as u32).to_be_bytes());
        b.extend(path);
    }
    b.extend([1, 1, 1]);
    b.extend(size.to_be_bytes());
    b.extend(hash);
    b.push(1);
    b.extend(0o644u32.to_be_bytes());
    b.push(1);
    b.extend((-4781i64).to_be_bytes());
    b.extend(999999999u32.to_be_bytes());
    b
}
pub fn cases() -> (Vec<Record>, Vec<Vec<u8>>) {
    let default_manifest = |size, hash| nested(&[255, 128, 1], &[254, 129], size, hash);
    let base = Record {
        manifest: default_manifest(3, [255; 32]),
        manifest_version: None,
        phase: "receiving".into(),
        normalized_path: vec![250, 11, 1],
        parent: [0, u64::MAX],
        private: [3, 4],
        data: [5, 6],
        durable_offset: 0,
        prefix_sha256: sum(&[]),
        last_stream: [20; 16],
        created: -77,
        updated: 0,
        expires: 604860,
        committed_final: None,
    };
    let high = (i64::MAX as u64) + 1;
    let mut values = vec![
        base.clone(),
        Record {
            manifest_version: Some(0),
            normalized_path: vec![],
            durable_offset: high,
            ..base.clone()
        },
        Record {
            normalized_path: vec![],
            durable_offset: high,
            ..base.clone()
        },
        Record {
            durable_offset: high,
            ..base.clone()
        },
    ];
    for phase in PHASES {
        for offset in [0, 1, 2, 3, 4, i64::MAX as u64] {
            for hash in [sum(&[]), [11; 32], [255; 32]] {
                for final_id in [None, Some(base.data), Some([0, 0])] {
                    values.push(Record {
                        phase: phase.into(),
                        durable_offset: offset,
                        prefix_sha256: hash,
                        committed_final: final_id,
                        ..base.clone()
                    });
                }
            }
        }
    }
    for path in [vec![], vec![0], vec![255], vec![1; 4104], vec![2; 4105]] {
        values.push(Record {
            normalized_path: path,
            ..base.clone()
        })
    }
    for (created, updated, expires) in [
        (-100, -1, i64::MAX),
        (i64::MIN, 0, i64::MIN),
        (-20, -20, 604840),
        (-20, -20, 604839),
        (1, 0, 604860),
        (i64::MIN, i64::MIN, i64::MIN + 604860),
        (i64::MAX, i64::MAX, i64::MAX),
        (i64::MIN, 1, i64::MIN),
    ] {
        values.push(Record {
            created,
            updated,
            expires,
            ..base.clone()
        });
    }
    values.push(Record {
        manifest: nested(&vec![8; 4096], &vec![9; 4096], 3, [255; 32]),
        normalized_path: vec![7; 4104],
        phase: "committed".into(),
        durable_offset: 3,
        prefix_sha256: [255; 32],
        committed_final: Some(base.data),
        ..base.clone()
    });
    for hash in [sum(&[]), [77; 32]] {
        values.push(Record {
            manifest: default_manifest(0, hash),
            ..base.clone()
        });
    }
    let sample = base.encode(DOMAIN);
    let body = &sample[..sample.len() - 32];
    let mut wires: Vec<_> = values.iter().map(|r| r.encode(DOMAIN)).collect();
    wires.extend((0..sample.len()).map(|n| sample[..n].to_vec()));
    wires.extend((0..body.len()).map(|n| seal(&body[..n], DOMAIN)));
    for i in 0..sample.len() {
        let mut b = sample.clone();
        b[i] ^= 1;
        wires.push(b);
    }
    wires.extend([
        vec![0; LIMIT + 1],
        vec![0; LIMIT],
        seal(&vec![0; LIMIT - 32], DOMAIN),
    ]);
    let mut trailing = body.to_vec();
    trailing.push(0);
    wires.push(seal(&trailing, DOMAIN));
    for n in [LIMIT + 1, LIMIT] {
        wires.push(seal(
            &[1u32.to_be_bytes(), (n as u32).to_be_bytes()].concat(),
            DOMAIN,
        ));
    }
    wires.push(seal(&2u32.to_be_bytes(), DOMAIN));
    let mut invalid = base.manifest.clone();
    invalid[..4].fill(0);
    let mut nested = 1u32.to_be_bytes().to_vec();
    nested.extend((invalid.len() as u32).to_be_bytes());
    nested.extend(invalid);
    wires.push(seal(&nested, DOMAIN));
    let phase = 8 + base.manifest.len();
    for tag in 0..=255 {
        for index in [phase, body.len() - 1] {
            let mut b = body.to_vec();
            b[index] = tag;
            wires.push(seal(&b, DOMAIN));
        }
    }
    let mut seen = BTreeSet::new();
    wires.retain(|b| seen.insert(b.clone()));
    (values, wires)
}

pub fn expectations(
    records: &[Record],
    wires: &[Vec<u8>],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, usize) {
    use crate::oracle::{decode, fault};
    use serde_json::json;
    let mut requests = Vec::new();
    let mut expected = Vec::new();
    for r in records {
        requests.push(json!({"operation":"construct","record":r}));
        let result = match r.validate() {
            Ok(()) => json!({"validation":{"ok":null},"encoding":{"ok":r.encode(DOMAIN)}}),
            Err(e) => json!({"validation":fault(e),"encoding":fault(e)}),
        };
        expected.push(result);
    }
    let mut rejected = 0;
    for w in wires {
        requests.push(json!({"operation":"decode","bytes":w}));
        expected.push(match decode(w) {
            Ok(record) => json!({"ok":record}),
            Err(e) => {
                rejected += 1;
                fault(e)
            }
        });
    }
    (requests, expected, rejected)
}
