// The same public API observer runs against both libraries.
use marmish_stream::{
    copy_protocol::{Direction, Manifest, Mtime},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize, Serialize)]
struct Timestamp {
    seconds: i64,
    nanos: u32,
}

#[derive(Deserialize, Serialize)]
struct Record {
    version: u32,
    transfer: [u8; 32],
    client: [u8; 32],
    server: [u8; 32],
    direction: Direction,
    source: Vec<u8>,
    destination: Vec<u8>,
    overwrite: bool,
    preserve_mode: bool,
    preserve_mtime: bool,
    size: u64,
    sha256: [u8; 32],
    mode: Option<u32>,
    mtime: Option<Timestamp>,
}

impl From<Record> for Manifest {
    fn from(r: Record) -> Self {
        Self {
            version: r.version,
            transfer: r.transfer,
            client: r.client,
            server: r.server,
            direction: r.direction,
            source: r.source,
            destination: r.destination,
            overwrite: r.overwrite,
            preserve_mode: r.preserve_mode,
            preserve_mtime: r.preserve_mtime,
            size: r.size,
            sha256: r.sha256,
            mode: r.mode,
            mtime: r.mtime.map(|m| Mtime {
                seconds: m.seconds,
                nanos: m.nanos,
            }),
        }
    }
}

impl From<Manifest> for Record {
    fn from(value: Manifest) -> Self {
        let Manifest {
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
        } = value;
        Self {
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
            mtime: mtime.map(|Mtime { seconds, nanos }| Timestamp { seconds, nanos }),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    Construct { manifest: Record },
    Decode { bytes: Vec<u8> },
}

fn report<T>(result: Result<T>, success: impl FnOnce(T) -> Value) -> Value {
    match result {
        Ok(value) => json!({"ok":success(value)}),
        Err(error) => json!({"error":[format!("{error:?}"),error.to_string()]}),
    }
}

fn main() {
    let _: fn(&Manifest) -> Result<Vec<u8>> = Manifest::encode;
    let _: fn(&Manifest) -> Result<()> = Manifest::validate;
    let _: fn(&Manifest) -> String = Manifest::namespace;
    let _: fn(&[u8]) -> Result<Manifest> = Manifest::decode;
    fn metadata_shape<T: Copy + Eq + std::fmt::Debug>() {}
    metadata_shape::<Mtime>();
    for line in io::stdin().lock().lines() {
        let request = serde_json::from_str::<Request>(&line.unwrap()).unwrap();
        let output = match request {
            Request::Construct { manifest } => {
                let value = Manifest::from(manifest);
                assert!(value == value.clone());
                json!({ "validation": report(value.validate(), |()| Value::Null),
                        "encode": report(value.encode(), |bytes| json!(bytes)), "namespace": value.namespace()})
            }
            Request::Decode { bytes } => report(Manifest::decode(&bytes), |manifest| {
                let namespace = manifest.namespace();
                json!({"manifest": Record::from(manifest), "namespace": namespace})
            }),
        };
        println!("{output}");
    }
}
