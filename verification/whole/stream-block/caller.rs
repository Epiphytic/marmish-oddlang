// Independent consumer for both native codecs. It writes ordinary wire records for its peer.
use marmish_stream::codec::{Codec, EncodedBlock, decode, encode};
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn codec(label: &Value) -> Codec {
    serde_json::from_value(label.clone()).unwrap()
}

fn answer(request: &Value) -> Value {
    let mode = codec(&request["negotiated"]);
    match request["operation"].as_str().unwrap() {
        "declarations" => {
            use marmish_stream::{
                Error, MAX_BLOCK, MAX_CONTENT, MAX_ENCODED, MAX_OFFSET, MAX_RECORDS, WINDOW_BYTES,
            };
            let errors = [
                Error::Invalid("sentinel"),
                Error::Codec,
                Error::Limit,
                Error::Offset,
                Error::Conflict,
                Error::Gap,
                Error::OldRetry,
                Error::Eof,
                Error::Ack,
            ];
            let rendered: Vec<_> = errors
                .iter()
                .map(|e| {
                    json!([
                        format!("{e:?}"),
                        e.to_string(),
                        std::error::Error::source(e).is_some()
                    ])
                })
                .collect();
            let limits = json!([
                MAX_BLOCK,
                MAX_ENCODED,
                MAX_CONTENT,
                WINDOW_BYTES,
                MAX_OFFSET,
                MAX_RECORDS
            ]);
            json!({"errors":rendered,"limits":limits})
        }
        "encode" => {
            let input: Vec<u8> = serde_json::from_value(request["bytes"].clone()).unwrap();
            match encode(request["offset"].as_u64().unwrap(), &input, mode) {
                Ok(block) => json!({"wire":serde_json::to_value(block).unwrap()}),
                Err(e) => json!({"error":format!("{e:?}"),"display":e.to_string()}),
            }
        }
        "decode" => {
            let block: EncodedBlock = match serde_json::from_value(request["wire"].clone()) {
                Ok(value) => value,
                Err(e) => return json!({"wireError":e.to_string()}),
            };
            let valid = block.validate().map_err(|e| format!("{e:?}"));
            let end = block.end().map_err(|e| format!("{e:?}"));
            let result = decode(&block, mode).map_err(|e| (format!("{e:?}"), e.to_string()));
            json!({"end":end,"valid":valid,"decoded":result})
        }
        _ => unreachable!(),
    }
}

fn main() {
    for row in io::stdin().lock().lines() {
        let (id, request): (String, Value) = serde_json::from_str(&row.unwrap()).unwrap();
        println!("{}", json!([id, answer(&request)]));
    }
}
