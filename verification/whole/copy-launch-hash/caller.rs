// This caller is compiled unchanged against the original and generated libraries.
use marmish_stream::{
    codec::Codec,
    copy_protocol::{AttemptMode, Direction, Launch},
    Error,
};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Request {
    version: u32,
    mode: AttemptMode,
    transfer: [u8; 32],
    client: [u8; 32],
    server: [u8; 32],
    direction: Direction,
    source: Vec<u8>,
    destination: Vec<u8>,
    overwrite: bool,
    preserve_mode: bool,
    preserve_mtime: bool,
    codec: Codec,
}

fn main() {
    let digest: fn(&Launch, Codec) -> Result<[u8; 32], Error> = Launch::digest;
    let namespace: fn(&Launch) -> String = Launch::namespace;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        let launch = Launch {
            version: request.version,
            mode: request.mode,
            transfer: request.transfer,
            client: request.client,
            server: request.server,
            direction: request.direction,
            source: request.source,
            destination: request.destination,
            overwrite: request.overwrite,
            preserve_mode: request.preserve_mode,
            preserve_mtime: request.preserve_mtime,
        };
        let result = match digest(&launch, request.codec) {
            Ok(bytes) => json!({"ok": bytes}),
            Err(error) => json!({"error": [format!("{error:?}"), error.to_string()]}),
        };
        println!(
            "{}",
            json!({"digest": result, "namespace": namespace(&launch)})
        );
    }
}
