use super::{DOMAIN, ENTRYPOINT, MAX, MIN, Record};
use std::collections::HashSet;
use stream_wire_replay::native::Random;

fn replace(value: &[u8], offset: usize, fragment: &[u8]) -> Vec<u8> {
    let mut out = value[..offset].to_vec();
    out.extend(fragment);
    out.extend(&value[(offset + fragment.len()).min(value.len())..]);
    out
}
fn id<const N: usize>(rand: &mut Random) -> [u8; N] {
    rand.bytes(N).try_into().unwrap()
}

pub(super) fn cases() -> (Vec<Record>, Vec<Vec<u8>>) {
    let mut rng = Random::new(8800943);
    let base = Record {
        context_version: 1,
        helper_protocol: 1,
        client: std::array::from_fn(|i| i as u8),
        server: std::array::from_fn(|i| 32 + i as u8),
        group: vec![0, 255, 128],
        session: [42; 16],
        stream: [24; 16],
        transfer: [12; 32],
        direction: "put",
        launch_digest: [37; 32],
        retention_seconds: MIN,
        expected_version: "v1_.a".into(),
        expected_target: "aarch64-apple-darwin".into(),
        expected_commit: [111; 20],
    };
    macro_rules! record {
        ($($key:ident : $val:expr),* $(,)?) => { Record { $($key:$val,)* ..base.clone() } };
    }
    let mut records = vec![base.clone(), record!(direction:"get")];
    for context in [true, false] {
        for v in [0, 2, u32::MAX] {
            records.push(if context {
                record!(context_version:v)
            } else {
                record!(helper_protocol:v)
            });
        }
    }
    for v in [0, MIN - 1, MIN + 1, MAX, MAX + 1, u64::MAX] {
        records.push(record!(retention_seconds:v));
    }
    for len in [0, 1, 1023, 1024, 1025] {
        records.push(record!(group:[0,255].into_iter().cycle().take(len).collect()));
    }
    for version in [true, false] {
        for name in [
            "".into(),
            "a".into(),
            "A-a_0.".into(),
            ".".into(),
            "A".repeat(63),
            "B".repeat(64),
            "a".repeat(65),
            "v 1".into(),
            "v/1".into(),
            "v+1".into(),
            "é".into(),
            "v\0x".into(),
        ] {
            records.push(if version {
                record!(expected_version:name)
            } else {
                record!(expected_target:name)
            });
        }
    }
    for n in 0..64 {
        records.push(record!(client:id(&mut rng),server:id(&mut rng),session:id(&mut rng),stream:id(&mut rng),transfer:id(&mut rng),launch_digest:id(&mut rng),expected_commit:id(&mut rng),direction:if n%2==0{"put"}else{"get"},group:rng.bytes(1+n),retention_seconds:MIN+rng.below((MAX-MIN+1) as u32) as u64));
    }
    let sample = base.encode(false);
    let largest =
        record!(group:vec![0;1024],expected_version:"a".repeat(64),expected_target:"z".repeat(64));
    let maximum = largest.encode(false);
    records.push(largest);
    let mut wires: Vec<_> = records.iter().map(|r| r.encode(false)).collect();
    for v in [&sample, &maximum] {
        wires.extend((0..v.len()).map(|i| v[..i].to_vec()));
    }
    for (len, byte) in [(sample.len() + 1, 0), (4096, b'x'), (4097, b'x')] {
        let mut w = sample.clone();
        w.resize(len, byte);
        wires.push(w);
    }
    let group = DOMAIN.len() + 8 + 64;
    let tag = group + 4 + base.group.len() + 16 + 16 + 32;
    let label = tag + 1 + 32 + 8;
    let target = label + 4 + base.expected_version.len();
    let trailer = sample.len() - ENTRYPOINT.len() - 4;
    for at in [group, label, target, trailer] {
        for len in [0u32, 64, 65, 1024, 1025, u32::MAX] {
            for raw in [&sample[..], &sample[..at + 4]] {
                wires.push(replace(raw, at, &len.to_be_bytes()));
            }
        }
    }
    for byte in 0..=255 {
        wires.push(replace(&sample, tag, &[byte]));
    }
    for at in [label, target] {
        for byte in [0, 10, 32, 43, 47, 127, 128, 255] {
            wires.push(replace(&sample, at + 4, &[byte]));
        }
    }
    wires.push(replace(&sample, trailer + 4, b"?"));
    wires.push(replace(&sample, 0, b"?"));
    for bad in [
        replace(&sample, DOMAIN.len(), &[255]),
        replace(&sample, DOMAIN.len() + 4, &2u32.to_be_bytes()),
        replace(&sample, label - 8, &0u64.to_be_bytes()),
    ] {
        wires.extend([
            replace(&bad, tag, &[255]),
            [bad.as_slice(), &[255]].concat(),
            replace(&bad, label + 4, &[255]),
            replace(&bad, trailer + 4, b"?"),
        ]);
    }
    for mut raw in [
        replace(&sample, trailer + 4, b"?"),
        replace(&sample, label + 4, &[255]),
        replace(&sample, tag, &[255]),
        record!(group:vec![]).encode(false),
    ] {
        raw.push(0);
        wires.push(raw);
    }
    for _ in 0..128 {
        let offset = rng.below(sample.len() as u32);
        let len = 1 + rng.below(6);
        wires.push(replace(&sample, offset, &rng.bytes(len)));
    }
    let mut seen = HashSet::new();
    wires.retain(|raw| seen.insert(raw.clone()));
    (records, wires)
}
