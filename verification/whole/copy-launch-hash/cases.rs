//! Standalone byte oracle for the Launch digest and state namespace.
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use stream_wire_replay::native::Random;

#[derive(Clone, Serialize)]
struct Request {
    version: u32,
    mode: &'static str,
    transfer: [u8; 32],
    client: [u8; 32],
    server: [u8; 32],
    direction: &'static str,
    source: Vec<u8>,
    destination: Vec<u8>,
    overwrite: bool,
    preserve_mode: bool,
    preserve_mtime: bool,
    codec: &'static str,
}

impl Request {
    fn expected(&self, swapped: bool) -> Value {
        let direction = if self.direction == "put" { 1 } else { 2 };
        let mut namespace = Sha256::new();
        namespace.update(b"marmish-copy-state/1\0");
        let principals = if swapped {
            [self.server, self.client]
        } else {
            [self.client, self.server]
        };
        for id in principals {
            namespace.update(id);
        }
        namespace.update(self.transfer);
        namespace.update([direction]);
        let namespace = format!(".marmish-copy-v1-{:x}", namespace.finalize());

        let paths = [&self.source, &self.destination];
        let error = if self.version != 1 {
            Some("copy_version")
        } else if paths
            .iter()
            .any(|p| !(1..=4096).contains(&p.len()) || p.contains(&0))
        {
            Some("copy_path")
        } else {
            None
        };
        let digest = if let Some(code) = error {
            json!({"error":[format!("Invalid({code:?})"),format!("invalid stream input: {code}")]})
        } else {
            let mut hash = Sha256::new();
            hash.update(b"marmish-copy-launch/1\0");
            hash.update(self.version.to_be_bytes());
            hash.update([if self.mode == "create" { 1 } else { 2 }]);
            let identities = if swapped {
                [self.client, self.transfer, self.server]
            } else {
                [self.transfer, self.client, self.server]
            };
            for id in identities {
                hash.update(id);
            }
            hash.update([direction]);
            for path in paths {
                hash.update((path.len() as u32).to_be_bytes());
                hash.update(path);
            }
            hash.update([
                self.overwrite as u8,
                self.preserve_mode as u8,
                self.preserve_mtime as u8,
                (self.codec == "zlib") as u8,
            ]);
            json!({"ok":hash.finalize().to_vec()})
        };
        json!({"digest":digest,"namespace":namespace})
    }
}

fn cases() -> Vec<Request> {
    let base = Request {
        version: 1,
        mode: "create",
        transfer: std::array::from_fn(|i| i as u8),
        client: std::array::from_fn(|i| 64 + i as u8),
        server: std::array::from_fn(|i| 128 + i as u8),
        direction: "put",
        source: vec![255, 128, 65],
        destination: vec![1, 2],
        overwrite: false,
        preserve_mode: false,
        preserve_mtime: false,
        codec: "none",
    };
    let mut cases = Vec::new();
    for mode in ["create", "resume"] {
        for direction in ["put", "get"] {
            for codec in ["none", "zlib"] {
                for bits in 0..8 {
                    cases.push(Request {
                        mode,
                        direction,
                        codec,
                        overwrite: bits & 4 != 0,
                        preserve_mode: bits & 2 != 0,
                        preserve_mtime: bits & 1 != 0,
                        ..base.clone()
                    });
                }
            }
        }
    }
    for i in 0..3 {
        let mut req = base.clone();
        [&mut req.transfer, &mut req.client, &mut req.server][i].reverse();
        cases.push(req);
    }
    for source in [true, false] {
        for path in [
            vec![],
            vec![0],
            vec![1, 0],
            vec![255],
            vec![254; 4095],
            vec![255; 4096],
            vec![255; 4097],
        ] {
            for version in [0, 1] {
                let mut req = Request {
                    version,
                    ..base.clone()
                };
                *if source {
                    &mut req.source
                } else {
                    &mut req.destination
                } = path.clone();
                cases.push(req);
            }
        }
    }
    for version in [0, 1, 2, u32::MAX] {
        cases.push(Request {
            version,
            source: vec![],
            destination: vec![0],
            ..base.clone()
        });
    }
    let mut random = Random::new(535492);
    for _ in 0..128 {
        let mut id = || random.bytes(32).try_into().unwrap();
        let (transfer, client, server) = (id(), id(), id());
        let mut path = || {
            (0..=random.below(96))
                .map(|_| 1 + random.below(255) as u8)
                .collect()
        };
        cases.push(Request {
            transfer,
            client,
            server,
            source: path(),
            destination: path(),
            ..cases[random.below(64)].clone()
        });
    }
    cases
}

pub fn rows() -> Vec<Value> {
    cases()
        .into_iter()
        .map(|r| json!({"request":r,"expected":r.expected(false),"swapped":r.expected(true)}))
        .collect()
}
