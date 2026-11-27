//! Independent, big-endian checkpoint format and its nested manifest constraints.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub const LIMIT: usize = 16_384;
pub const DOMAIN: &[u8] = b"marmish-copy-checkpoint/1\0";
pub const PHASES: [&str; 5] = [
    "created",
    "receiving",
    "verifying",
    "commit_prepared",
    "committed",
];
const RETENTION: i64 = 604_860;
pub type Check<T> = std::result::Result<T, &'static str>;
pub type Id = [u64; 2];
pub fn fault(name: &str) -> Value {
    match name {
        "Offset" => json!({"error":["Offset","invalid stream offset"]}),
        "Limit" => json!({"error":["Limit","stream capacity reached"]}),
        _ => {
            json!({"error":[format!("Invalid(\"{name}\")"),format!("invalid stream input: {name}")]})
        }
    }
}
pub fn sum(input: &[u8]) -> [u8; 32] {
    Sha256::digest(input).into()
}
pub fn seal(body: &[u8], domain: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(body);
    let mut w = body.to_vec();
    w.extend(h.finalize());
    w
}
struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Check<&'a [u8]> {
        let (v, rest) = self.0.split_at_checked(n).ok_or("copy_truncated")?;
        self.0 = rest;
        Ok(v)
    }
    fn array<const N: usize>(&mut self) -> Check<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn byte(&mut self) -> Check<u8> {
        Ok(self.array::<1>()?[0])
    }
    fn u32(&mut self) -> Check<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    fn u64(&mut self) -> Check<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    fn i64(&mut self) -> Check<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }
    fn id(&mut self) -> Check<Id> {
        Ok([self.u64()?, self.u64()?])
    }
    fn bytes(&mut self, limit: usize) -> Check<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > limit {
            return Err("Limit");
        }
        Ok(self.take(len)?.to_vec())
    }
    fn flag(&mut self, error: &'static str) -> Check<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(error),
        }
    }
    fn finish(&self) -> Check<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("copy_trailing")
        }
    }
}
fn manifest(bytes: &[u8]) -> Check<(u64, [u8; 32])> {
    if bytes.len() > LIMIT {
        return Err("Limit");
    }
    let mut c = Cursor(bytes);
    let version = c.u32()?;
    c.take(96)?;
    if !matches!(c.byte()?, 1 | 2) {
        return Err("copy_enum");
    }
    let src = c.bytes(4096)?;
    let dst = c.bytes(4096)?;
    c.flag("copy_boolean")?;
    let pm = c.flag("copy_boolean")?;
    let pt = c.flag("copy_boolean")?;
    let size = c.u64()?;
    if size > i64::MAX as u64 {
        return Err("Offset");
    }
    let hash = c.array()?;
    let mode = if c.flag("copy_option")? {
        Some(c.u32()?)
    } else {
        None
    };
    let mtime = if c.flag("copy_option")? {
        Some((c.i64()?, c.u32()?))
    } else {
        None
    };
    c.finish()?;
    if version != 1 {
        return Err("copy_version");
    }
    if src.is_empty() || dst.is_empty() || src.contains(&0) || dst.contains(&0) {
        return Err("copy_path");
    }
    if pm != mode.is_some()
        || pt != mtime.is_some()
        || mode.is_some_and(|m| m & !0o777 != 0 || m & 0o600 != 0o600)
        || mtime.is_some_and(|(_, n)| n >= 1_000_000_000)
    {
        return Err("copy_metadata");
    }
    Ok((size, hash))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub manifest: Vec<u8>,
    pub manifest_version: Option<u32>,
    pub phase: String,
    pub normalized_path: Vec<u8>,
    pub parent: Id,
    pub private: Id,
    pub data: Id,
    pub durable_offset: u64,
    pub prefix_sha256: [u8; 32],
    pub last_stream: [u8; 16],
    pub created: i64,
    pub updated: i64,
    pub expires: i64,
    pub committed_final: Option<Id>,
}
impl Record {
    pub fn validate(&self) -> Check<()> {
        let (size, hash) = manifest(&self.manifest)?;
        if self.manifest_version.is_some_and(|v| v != 1) {
            return Err("copy_version");
        }
        if self.normalized_path.is_empty()
            || self.normalized_path.len() > 4104
            || self.normalized_path.contains(&0)
        {
            return Err("copy_local_path");
        }
        let offset = self.durable_offset;
        let phase = self.phase.as_str();
        let final_id = self.committed_final;
        if offset > size
            || self.updated < self.created
            || self
                .expires
                .checked_sub(self.updated)
                .is_none_or(|a| a < RETENTION)
            || (phase == "created" && offset != 0)
            || (PHASES[2..].contains(&phase) && offset != size)
            || (phase == "committed") != final_id.is_some()
            || final_id.is_some_and(|id| id != self.data)
            || (offset == 0 && self.prefix_sha256 != sum(&[]))
            || (offset == size && self.prefix_sha256 != hash)
        {
            return Err("copy_checkpoint");
        }
        Ok(())
    }
    pub fn encode(&self, domain: &[u8]) -> Vec<u8> {
        let mut body = 1u32.to_be_bytes().to_vec();
        let mut m = self.manifest.clone();
        if let Some(v) = self.manifest_version {
            m[..4].copy_from_slice(&v.to_be_bytes())
        }
        let put = |v: &mut Vec<u8>, p: &[u8]| {
            v.extend((p.len() as u32).to_be_bytes());
            v.extend_from_slice(p)
        };
        put(&mut body, &m);
        body.push(1 + PHASES.iter().position(|p| *p == self.phase).unwrap() as u8);
        put(&mut body, &self.normalized_path);
        for id in [self.parent, self.private, self.data] {
            for n in id {
                body.extend(n.to_be_bytes());
            }
        }
        body.extend(self.durable_offset.to_be_bytes());
        body.extend(self.prefix_sha256);
        body.extend(self.last_stream);
        for t in [self.created, self.updated, self.expires] {
            body.extend(t.to_be_bytes())
        }
        body.push(u8::from(self.committed_final.is_some()));
        if let Some(id) = self.committed_final {
            for n in id {
                body.extend(n.to_be_bytes())
            }
        }
        seal(&body, domain)
    }
}
pub fn decode(wire: &[u8]) -> Check<Record> {
    if !(32..=LIMIT).contains(&wire.len()) {
        return Err("Limit");
    }
    let (body, check) = wire.split_at(wire.len() - 32);
    if &seal(body, DOMAIN)[body.len()..] != check {
        return Err("copy_checkpoint_checksum");
    }
    let mut c = Cursor(body);
    if c.u32()? != 1 {
        return Err("copy_checkpoint_version");
    }
    let nested = c.bytes(LIMIT)?;
    manifest(&nested)?;
    let phase = c.byte()?;
    if !(1..=5).contains(&phase) {
        return Err("copy_enum");
    }
    let path = c.bytes(4104)?;
    let parent = c.id()?;
    let private = c.id()?;
    let data = c.id()?;
    let offset = c.u64()?;
    if offset > i64::MAX as u64 {
        return Err("Offset");
    }
    let record = Record {
        manifest: nested,
        manifest_version: None,
        phase: PHASES[usize::from(phase) - 1].into(),
        normalized_path: path,
        parent,
        private,
        data,
        durable_offset: offset,
        prefix_sha256: c.array()?,
        last_stream: c.array()?,
        created: c.i64()?,
        updated: c.i64()?,
        expires: c.i64()?,
        committed_final: if c.flag("copy_option")? {
            Some(c.id()?)
        } else {
            None
        },
    };
    c.finish()?;
    record.validate()?;
    Ok(record)
}
