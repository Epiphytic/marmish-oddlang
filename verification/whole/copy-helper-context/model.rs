use crate::{Cases, cursor::Cursor, manifest::fault};
use serde::Serialize;
use serde_json::json;
#[path = "corpus.rs"]
mod corpus;

const DOMAIN: &[u8] = b"marmish-copy-context/1\0";
const ENTRYPOINT: &[u8] = b"__copy-helper-v1";
const MIN: u64 = 604860;
const MAX: u64 = 31536000;
type Check<T> = Result<T, &'static str>;

#[derive(Clone, PartialEq, Debug, Serialize)]
struct Record {
    context_version: u32,
    helper_protocol: u32,
    client: [u8; 32],
    server: [u8; 32],
    group: Vec<u8>,
    session: [u8; 16],
    stream: [u8; 16],
    transfer: [u8; 32],
    direction: &'static str,
    launch_digest: [u8; 32],
    retention_seconds: u64,
    expected_version: String,
    expected_target: String,
    expected_commit: [u8; 20],
}

fn atom(value: &[u8]) -> bool {
    (1..=64).contains(&value.len())
        && value
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(c))
}
fn read_atom(c: &mut Cursor) -> Check<String> {
    let v = c.bytes(64)?;
    if !atom(&v) {
        return Err("copy_context_build");
    }
    Ok(String::from_utf8(v).unwrap())
}
fn framed(out: &mut Vec<u8>, data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
}
impl Record {
    fn valid(&self) -> bool {
        self.context_version == 1
            && self.helper_protocol == 1
            && (1..=1024).contains(&self.group.len())
            && (MIN..=MAX).contains(&self.retention_seconds)
            && atom(self.expected_version.as_bytes())
            && atom(self.expected_target.as_bytes())
    }
    fn encode(&self, swapped: bool) -> Vec<u8> {
        let mut out = DOMAIN.to_vec();
        out.extend(self.context_version.to_be_bytes());
        out.extend(self.helper_protocol.to_be_bytes());
        for id in if swapped {
            [self.server, self.client]
        } else {
            [self.client, self.server]
        } {
            out.extend(id);
        }
        framed(&mut out, &self.group);
        out.extend(self.session);
        out.extend(self.stream);
        out.extend(self.transfer);
        out.push(if self.direction == "put" { 1 } else { 2 });
        out.extend(self.launch_digest);
        out.extend(self.retention_seconds.to_be_bytes());
        framed(&mut out, self.expected_version.as_bytes());
        framed(&mut out, self.expected_target.as_bytes());
        out.extend(self.expected_commit);
        framed(&mut out, ENTRYPOINT);
        out
    }
    fn decode(data: &[u8]) -> Check<Self> {
        if data.len() > 4096 {
            return Err("copy_context");
        }
        let mut c = Cursor(data.strip_prefix(DOMAIN).ok_or("copy_context")?);
        let r = Self {
            context_version: c.u32()?,
            helper_protocol: c.u32()?,
            client: c.array()?,
            server: c.array()?,
            group: c.bytes(1024)?,
            session: c.array()?,
            stream: c.array()?,
            transfer: c.array()?,
            direction: match c.byte()? {
                1 => "put",
                2 => "get",
                _ => return Err("copy_enum"),
            },
            launch_digest: c.array()?,
            retention_seconds: c.u64()?,
            expected_version: read_atom(&mut c)?,
            expected_target: read_atom(&mut c)?,
            expected_commit: c.array()?,
        };
        if c.bytes(64)? != ENTRYPOINT {
            return Err("copy_context_entrypoint");
        }
        c.finish()?;
        if !r.valid() {
            return Err("copy_context");
        }
        Ok(r)
    }
}

pub fn cases() -> Cases {
    let (records, wires) = corpus::cases();
    let mut requests = vec![];
    let mut expected = vec![];
    for r in &records {
        requests.push(json!({"operation":"construct","record":r}));
        let (valid, encode) = if r.valid() {
            let bytes = r.encode(false);
            assert_eq!(Record::decode(&bytes).as_ref(), Ok(r));
            (json!({"ok":null}), json!({"ok":bytes}))
        } else {
            (fault("copy_context"), fault("copy_context"))
        };
        expected.push(json!({"validation":valid,"encode":encode}));
    }
    let mut rejected = 0;
    for w in &wires {
        requests.push(json!({"operation":"decode","bytes":w}));
        expected.push(match Record::decode(w) {
            Ok(r) => json!({"ok":r}),
            Err(e) => {
                rejected += 1;
                fault(e)
            }
        });
    }
    let swapped = json!({"validation":{"ok":null},"encode":{"ok":records[0].encode(true)}});
    assert_ne!(swapped, expected[0]);
    Cases {
        mutant: vec![(requests[0].clone(), swapped)],
        requests,
        expected,
        metadata: json!({"constructionCases":records.len(),"decodeCases":wires.len(),"rejectedDecodeCases":rejected}),
        negative: "repinned fixed-field permutation emits independently predicted swapped bytes",
    }
}
