// Unchanged public API observer for pinned original and generated library.
use marmish_stream::{
    copy_protocol::{Checkpoint, FileIdentity, Manifest, Phase},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize, Serialize)]
struct Record {
    manifest: Vec<u8>,
    manifest_version: Option<u32>,
    phase: Phase,
    normalized_path: Vec<u8>,
    parent: [u64; 2],
    private: [u64; 2],
    data: [u64; 2],
    durable_offset: u64,
    prefix_sha256: [u8; 32],
    last_stream: [u8; 16],
    created: i64,
    updated: i64,
    expires: i64,
    committed_final: Option<[u64; 2]>,
}

fn identity([device, inode]: [u64; 2]) -> FileIdentity {
    FileIdentity { device, inode }
}
fn pair(FileIdentity { device, inode }: FileIdentity) -> [u64; 2] {
    [device, inode]
}

impl From<Record> for Checkpoint {
    fn from(record: Record) -> Self {
        let mut manifest = Manifest::decode(&record.manifest).unwrap();
        if let Some(version) = record.manifest_version {
            manifest.version = version;
        }
        Self {
            manifest,
            phase: record.phase,
            normalized_path: record.normalized_path,
            parent: identity(record.parent),
            private: identity(record.private),
            data: identity(record.data),
            durable_offset: record.durable_offset,
            prefix_sha256: record.prefix_sha256,
            last_stream: record.last_stream,
            created: record.created,
            updated: record.updated,
            expires: record.expires,
            committed_final: record.committed_final.map(identity),
        }
    }
}

impl From<Checkpoint> for Record {
    fn from(value: Checkpoint) -> Self {
        let Checkpoint {
            manifest,
            phase,
            normalized_path,
            parent,
            private,
            data,
            durable_offset,
            prefix_sha256,
            last_stream,
            created,
            updated,
            expires,
            committed_final,
        } = value;
        Self {
            manifest: manifest.encode().unwrap(),
            manifest_version: None,
            phase,
            normalized_path,
            parent: pair(parent),
            private: pair(private),
            data: pair(data),
            durable_offset,
            prefix_sha256,
            last_stream,
            created,
            updated,
            expires,
            committed_final: committed_final.map(pair),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    Construct { record: Record },
    Decode { bytes: Vec<u8> },
}

fn report<T>(checked: Result<T>, value: impl FnOnce(T) -> Value) -> Value {
    match checked {
        Ok(result) => json!({"ok":value(result)}),
        Err(error) => json!({"error":[format!("{error:?}"),error.to_string()]}),
    }
}

fn main() {
    fn identity_traits<T: Copy + Eq + std::fmt::Debug>() {}
    identity_traits::<FileIdentity>();
    for line in io::stdin().lock().lines() {
        let request = serde_json::from_str::<Request>(&line.unwrap()).unwrap();
        let reply = match request {
            Request::Construct { record } => {
                let value = Checkpoint::from(record);
                assert!(value == value.clone());
                json!({"validation":report(value.validate(),|()|Value::Null),
                    "encoding":report(value.encode(),|bytes|json!(bytes))})
            }
            Request::Decode { bytes } => report(Checkpoint::decode(&bytes), |value| {
                json!(Record::from(value))
            }),
        };
        println!("{reply}");
    }
}
