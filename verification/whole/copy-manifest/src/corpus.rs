use super::manifest::{LIMIT, MAX, Record, Time, base};
use std::collections::HashSet;
use stream_wire_replay::native::Random;

pub fn cases() -> (Vec<Record>, Vec<Vec<u8>>) {
    let b = base();
    macro_rules! record {
        ($($field:ident: $value:expr),* $(,)?) => {
            Record { $($field: $value,)* ..b.clone() }
        };
    }
    let mut records = vec![
        b.clone(),
        record!(direction: "get".into()),
        record!(size: 0),
        record!(size: MAX+1),
        record!(size: u64::MAX),
        record!(version: 0),
        record!(version: u32::MAX),
        record!(version: 0, source: vec![], size: MAX+1),
        record!(source: vec![], size: MAX+1),
        record!(size: MAX+1, mode: Some(0o40)),
        record!(source: vec![0], mode: Some(0o40)),
        record!(destination: vec![], preserve_mtime: false),
        record!(version: 0, source: vec![0]),
    ];
    for m in (0..512).chain([512, 1024, u32::MAX]) {
        records.push(record!(mode: Some(m)))
    }
    for mode in [None, Some(0o600), Some(0o40)] {
        for preserve in [true, false] {
            records.push(record!(mode: mode, preserve_mode: preserve))
        }
    }
    let mut times = vec![None];
    for seconds in [i64::MIN, -1, 0, i64::MAX] {
        for nanos in [0, 999_999_999, 1_000_000_000, u32::MAX] {
            times.push(Some(Time { seconds, nanos }))
        }
    }
    for t in times {
        for preserve in [true, false] {
            records.push(record!(mtime: t.clone(), preserve_mtime: preserve))
        }
    }
    for src in [true, false] {
        for p in [
            vec![],
            vec![0],
            vec![255],
            vec![255, 0],
            vec![255; 4095],
            vec![1; 4096],
            vec![1; 4097],
        ] {
            records.push(if src {
                record!(source: p)
            } else {
                record!(destination: p)
            })
        }
    }
    records.push(record!(source: vec![254;4096], destination: vec![255;4096]));
    let mut rng = Random::new(2741);
    let mut id = || rng.bytes(32).try_into().unwrap();
    for i in 0..48 {
        let odd = i % 2 == 1;
        let pm = i % 3 != 0;
        records.push(Record {
            transfer: id(),
            client: id(),
            server: id(),
            sha256: id(),
            direction: if odd { "get" } else { "put" }.into(),
            overwrite: odd,
            preserve_mtime: odd,
            mtime: if odd { b.mtime.clone() } else { None },
            preserve_mode: pm,
            mode: pm.then_some(0o700),
            ..b.clone()
        });
    }
    let sample = b.encode();
    let mut wires: Vec<_> = records.iter().map(Record::encode).collect();
    wires.extend((0..sample.len()).map(|n| sample[..n].to_vec()));
    for (n, v) in [(sample.len() + 1, 0), (LIMIT, b'x'), (LIMIT + 1, b'x')] {
        let mut w = sample.clone();
        w.resize(n, v);
        wires.push(w)
    }
    let src = 4 + 3 * 32 + 1;
    let dst = src + 4 + b.source.len();
    let flags = dst + 4 + b.destination.len();
    let size = flags + 3;
    let mode = size + 8 + 32;
    let mtime = mode + 5;
    fn replace(raw: &[u8], at: usize, val: &[u8]) -> Vec<u8> {
        let mut v = raw.to_vec();
        v[at..at + val.len()].copy_from_slice(val);
        v
    }
    for at in [src, dst] {
        for n in [0u32, 1, 4096, 4097, u32::MAX] {
            for raw in [&sample[..], &sample[..at + 4]] {
                wires.push(replace(raw, at, &n.to_be_bytes()))
            }
        }
    }
    for at in [src - 1, flags, flags + 1, flags + 2, mode, mtime] {
        for byte in 0..=255 {
            wires.push(replace(&sample, at, &[byte]))
        }
    }
    let mut over = replace(&sample, size, &(MAX + 1).to_be_bytes());
    wires.push(over[..size + 8].to_vec());
    wires.push(replace(&over, mode, &[255]));
    over.push(255);
    wires.push(over);
    for r in [
        record!(version: 0),
        record!(source: vec![0]),
        record!(destination: vec![]),
        record!(mode: Some(0o40)),
        record!(mtime: Some(Time{seconds: 0,nanos: 1_000_000_000})),
    ] {
        let mut w = r.encode();
        w.push(0);
        wires.push(w)
    }
    let mut seen = HashSet::new();
    wires.retain(|w| seen.insert(w.clone()));
    (records, wires)
}
