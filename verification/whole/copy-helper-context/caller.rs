// Compile this unchanged against both libraries.
use marmish_stream::{
    copy_protocol::{Direction, HelperContext},
    Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Record {
    context_version: u32,
    helper_protocol: u32,
    client: [u8; 32],
    server: [u8; 32],
    group: Vec<u8>,
    session: [u8; 16],
    stream: [u8; 16],
    transfer: [u8; 32],
    direction: Direction,
    launch_digest: [u8; 32],
    retention_seconds: u64,
    expected_version: String,
    expected_target: String,
    expected_commit: [u8; 20],
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    Construct { record: Record },
    Decode { bytes: Vec<u8> },
}

fn report<T>(result: Result<T>, ok: impl FnOnce(T) -> Value) -> Value {
    match result {
        Ok(value) => json!({"ok":ok(value)}),
        Err(error) => json!({"error":[format!("{error:?}"),error.to_string()]}),
    }
}

fn main() {
    let validate: fn(&HelperContext) -> Result<()> = HelperContext::validate;
    let encode: fn(&HelperContext) -> Result<Vec<u8>> = HelperContext::encode;
    let decode: fn(&[u8]) -> Result<HelperContext> = HelperContext::decode;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        let reply = match request {
            Request::Construct { record } => {
                let context = HelperContext {
                    context_version: record.context_version,
                    helper_protocol: record.helper_protocol,
                    client: record.client,
                    server: record.server,
                    group: record.group,
                    session: record.session,
                    stream: record.stream,
                    transfer: record.transfer,
                    direction: record.direction,
                    launch_digest: record.launch_digest,
                    retention_seconds: record.retention_seconds,
                    expected_version: record.expected_version,
                    expected_target: record.expected_target,
                    expected_commit: record.expected_commit,
                };
                json!({
                    "validation": report(validate(&context), |()| Value::Null),
                    "encode": report(encode(&context), |bytes| json!(bytes)),
                })
            }
            Request::Decode { bytes } => report(decode(&bytes), |context| {
                let HelperContext {
                    context_version,
                    helper_protocol,
                    client,
                    server,
                    group,
                    session,
                    stream,
                    transfer,
                    direction,
                    launch_digest,
                    retention_seconds,
                    expected_version,
                    expected_target,
                    expected_commit,
                } = context;
                let _: (
                    u32,
                    u32,
                    [u8; 32],
                    [u8; 32],
                    [u8; 16],
                    [u8; 16],
                    [u8; 32],
                    [u8; 32],
                    u64,
                    [u8; 20],
                ) = (
                    context_version,
                    helper_protocol,
                    client,
                    server,
                    session,
                    stream,
                    transfer,
                    launch_digest,
                    retention_seconds,
                    expected_commit,
                );
                json!({ "context_version":context_version, "helper_protocol":helper_protocol,
                    "client":client, "server":server, "group":group, "session":session,
                    "stream":stream, "transfer":transfer, "direction":direction,
                    "launch_digest":launch_digest, "retention_seconds":retention_seconds,
                    "expected_version":expected_version, "expected_target":expected_target,
                    "expected_commit":expected_commit })
            }),
        };
        println!("{reply}");
    }
}
