use public_data_qualification::assembly::bytes_sha;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub const LIMIT: usize = 16384;
pub const MAX: u64 = i64::MAX as u64;
type Check<T> = std::result::Result<T, &'static str>;
pub fn fault(name: &str) -> Value {
    match name {
        "Offset" => json!({"error":["Offset","invalid stream offset"]}),
        "Limit" => json!({"error":["Limit","stream capacity reached"]}),
        _ => {
            json!({"error":[format!("Invalid(\"{name}\")"),format!("invalid stream input: {name}")]})
        }
    }
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
#[derive(Clone, Serialize, Deserialize)]
pub struct Time {
    pub seconds: i64,
    pub nanos: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    pub transfer: [u8; 32],
    pub client: [u8; 32],
    pub server: [u8; 32],
    pub direction: String,
    pub source: Vec<u8>,
    pub destination: Vec<u8>,
    pub overwrite: bool,
    pub preserve_mode: bool,
    pub preserve_mtime: bool,
    pub size: u64,
    pub sha256: [u8; 32],
    pub mode: Option<u32>,
    pub mtime: Option<Time>,
}
pub fn base() -> Record {
    Record {
        version: 1,
        transfer: std::array::from_fn(|i| i as u8),
        client: std::array::from_fn(|i| 32 + i as u8),
        server: std::array::from_fn(|i| 64 + i as u8),
        direction: "put".into(),
        source: vec![255, 128, 1],
        destination: vec![254, 129],
        overwrite: true,
        preserve_mode: true,
        preserve_mtime: true,
        size: MAX,
        sha256: [255; 32],
        mode: Some(0o644),
        mtime: Some(Time {
            seconds: -4781,
            nanos: 999_999_999,
        }),
    }
}
pub fn frame(out: &mut Vec<u8>, v: &[u8]) {
    out.extend((v.len() as u32).to_be_bytes());
    out.extend(v);
}
impl Record {
    pub fn header(&self) -> Check<()> {
        if self.version != 1 {
            return Err("copy_version");
        }
        if [&self.source, &self.destination]
            .into_iter()
            .any(|p| !(1..=4096).contains(&p.len()) || p.contains(&0))
        {
            return Err("copy_path");
        }
        Ok(())
    }
    pub fn validate(&self) -> Check<()> {
        self.header()?;
        if self.size > MAX {
            return Err("Offset");
        }
        if self.mode.is_some() != self.preserve_mode
            || self.mtime.is_some() != self.preserve_mtime
            || self
                .mode
                .is_some_and(|m| m & !0o777 != 0 || m & 0o600 != 0o600)
            || self
                .mtime
                .as_ref()
                .is_some_and(|t| t.nanos >= 1_000_000_000)
        {
            return Err("copy_metadata");
        }
        Ok(())
    }
    pub fn tag(&self) -> u8 {
        if self.direction == "put" { 1 } else { 2 }
    }
    pub fn fields(&self, out: &mut Vec<u8>) {
        for id in [&self.transfer, &self.client, &self.server] {
            out.extend(id)
        }
        out.push(self.tag());
        frame(out, &self.source);
        frame(out, &self.destination);
        out.extend([
            self.overwrite as u8,
            self.preserve_mode as u8,
            self.preserve_mtime as u8,
        ]);
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.version.to_be_bytes().to_vec();
        self.fields(&mut b);
        b.extend(self.size.to_be_bytes());
        b.extend(self.sha256);
        b.push(self.mode.is_some() as u8);
        if let Some(m) = self.mode {
            b.extend(m.to_be_bytes())
        }
        b.push(self.mtime.is_some() as u8);
        if let Some(t) = &self.mtime {
            b.extend(t.seconds.to_be_bytes());
            b.extend(t.nanos.to_be_bytes());
        }
        b
    }
    pub fn namespace(&self, swapped: bool) -> String {
        let mut b = b"marmish-copy-state/1\0".to_vec();
        let ids = if swapped {
            [self.server, self.client]
        } else {
            [self.client, self.server]
        };
        for id in ids {
            b.extend(id)
        }
        b.extend(self.transfer);
        b.push(self.tag());
        format!(".marmish-copy-v1-{}", bytes_sha(&b))
    }
    pub fn decode(raw: &[u8]) -> Check<Self> {
        if raw.len() > LIMIT {
            return Err("Limit");
        }
        let mut c = Cursor(raw);
        let version = c.u32()?;
        let transfer = c.array()?;
        let client = c.array()?;
        let server = c.array()?;
        let direction = match c.byte()? {
            1 => "put",
            2 => "get",
            _ => return Err("copy_enum"),
        }
        .into();
        let source = c.bytes(4096)?;
        let destination = c.bytes(4096)?;
        let overwrite = c.flag("copy_boolean")?;
        let preserve_mode = c.flag("copy_boolean")?;
        let preserve_mtime = c.flag("copy_boolean")?;
        let size = c.u64()?;
        if size > MAX {
            return Err("Offset");
        }
        let sha256 = c.array()?;
        let mode = if c.flag("copy_option")? {
            Some(c.u32()?)
        } else {
            None
        };
        let mtime = if c.flag("copy_option")? {
            Some(Time {
                seconds: c.i64()?,
                nanos: c.u32()?,
            })
        } else {
            None
        };
        c.finish()?;
        let r = Self {
            version,
            transfer,
            client,
            server,
            direction,
            source,
            destination,
            overwrite,
            preserve_mode,
            preserve_mtime,
            size,
            sha256,
            mode,
            mtime,
        };
        r.validate()?;
        Ok(r)
    }
}
