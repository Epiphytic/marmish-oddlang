//! Independent Launch construction and JSON decoding cases.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

fn request(
    version: u32,
    mode: &str,
    direction: &str,
    source: &[u8],
    destination: &[u8],
) -> (Value, Value) {
    let input = json!({"operation":"construct","version":version,"mode":mode,"direction":direction,"source":source,"destination":destination});
    let wire = json!({"version":version,"mode":mode,"transfer":"01".repeat(32),"client":"02".repeat(32),"server":"03".repeat(32),"direction":direction,"source":STANDARD.encode(source),"destination":STANDARD.encode(destination),"overwrite":true,"preserve_mode":false,"preserve_mtime":true});
    let error = if version != 1 {
        Some("copy_version")
    } else if [source, destination]
        .iter()
        .any(|p| !(1..=4096).contains(&p.len()) || p.contains(&0))
    {
        Some("copy_path")
    } else {
        None
    };
    let valid = match error {
        Some(code) => {
            json!({"Err":[format!("Invalid({code:?})"),format!("invalid stream input: {code}")]})
        }
        None => json!({"Ok":null}),
    };
    (input, json!({"wire":wire.to_string(),"valid":valid}))
}

pub fn constructs() -> (Vec<Value>, Vec<Value>) {
    let mut rows = Vec::new();
    for version in [0, 1, 2, u32::MAX] {
        for mode in ["create", "resume"] {
            for direction in ["put", "get"] {
                rows.push(request(version, mode, direction, &[1], &[255]));
            }
        }
    }
    for source in [true, false] {
        for p in [
            vec![],
            vec![0],
            vec![0, 2],
            vec![1, 0],
            vec![254; 4095],
            vec![255; 4096],
            vec![255; 4097],
        ] {
            for version in [0, 1] {
                let (src, dst): (&[u8], &[u8]) = if source { (&p, &[255]) } else { (&[1], &p) };
                rows.push(request(version, "create", "put", src, dst));
            }
        }
    }
    rows.push(request(1, "create", "put", &[], &[]));
    rows.into_iter().unzip()
}

pub fn decodes() -> Vec<Value> {
    let (_, example) = request(1, "create", "put", &[1], &[255]);
    let raw = example["wire"].as_str().unwrap();
    let wire: Map<String, Value> = serde_json::from_str(raw).unwrap();
    let reversed: Map<_, _> = wire.clone().into_iter().rev().collect();
    let sequence: Vec<_> = wire.values().cloned().collect();
    let mut extra = sequence.clone();
    extra.push(json!(true));
    let replace = |field: &str, value: Value| {
        let mut row = wire.clone();
        row.insert(field.into(), value);
        Value::Object(row).to_string()
    };
    let mut texts = vec![
        raw.into(),
        Value::Object(reversed).to_string(),
        json!(sequence).to_string(),
        json!(&sequence[..sequence.len() - 1]).to_string(),
        json!(extra).to_string(),
        format!("{{\"version\":1,{}", &raw[1..]),
        replace("extraneous", json!(true)),
        "null".into(),
        "{}".into(),
    ];
    for key in wire.keys() {
        let mut row = wire.clone();
        row.shift_remove(key);
        texts.push(Value::Object(row).to_string());
    }
    for key in ["transfer", "client", "server"] {
        for v in [
            json!("AA".repeat(32)),
            json!("xx".repeat(32)),
            json!("00".repeat(31)),
            json!("a0".repeat(32)),
            Value::Null,
        ] {
            texts.push(replace(key, v));
        }
    }
    for key in ["source", "destination"] {
        for v in [
            "".into(),
            "AA==".into(),
            "/w==".into(),
            "AB==".into(),
            "?".into(),
            "?".repeat(5465),
            STANDARD.encode([b'b'; 4096]),
            STANDARD.encode([b'b'; 4097]),
        ] {
            texts.push(replace(key, json!(v)));
        }
    }
    for (k, v) in [
        ("mode", json!("future")),
        ("direction", json!("future")),
        ("version", json!(-1)),
        ("version", json!(1u64 << 32)),
        ("overwrite", json!("true")),
    ] {
        texts.push(replace(k, v));
    }
    texts
        .into_iter()
        .map(|text| json!({"operation":"decode","text":text}))
        .collect()
}
