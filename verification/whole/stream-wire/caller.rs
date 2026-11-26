use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

// The structural slice uses the original implementations of referenced types.
pub use marmish_stream::{codec, copy_protocol, wire};
mod projected;

fn pairs<A, B>(values: &[Value]) -> usize
where
    A: DeserializeOwned + Serialize,
    B: DeserializeOwned + Serialize,
{
    for value in values {
        let value = serde_json::to_vec(value).unwrap();
        let original = serde_json::from_slice::<A>(&value).unwrap();
        let generated = serde_json::from_slice::<B>(&value).unwrap();
        let expected = serde_json::to_vec(&original).unwrap();
        assert_eq!(serde_json::to_vec(&generated).unwrap(), expected);
        let from_generated =
            serde_json::from_slice::<A>(&serde_json::to_vec(&generated).unwrap()).unwrap();
        assert_eq!(serde_json::to_vec(&from_generated).unwrap(), expected);
    }
    values.len()
}

fn rejected<A: DeserializeOwned, B: DeserializeOwned>(input: &str, prefix: Option<&str>) {
    let a = serde_json::from_str::<A>(input)
        .err()
        .expect("original accepted control");
    let b = serde_json::from_str::<B>(input)
        .err()
        .expect("projection accepted control");
    if let Some(prefix) = prefix {
        assert!(a.to_string().starts_with(prefix), "{a}");
        assert!(b.to_string().starts_with(prefix), "{b}");
    }
}

fn main() {
    use serde_json::json;
    let binding = json!({
        "stream": "10".repeat(16), "session": "20".repeat(16),
        "client": "30".repeat(32), "server": "40".repeat(32)
    });
    let request = [
        json!({"op":"hello","data":{"codecs":["none","zlib"]}}),
        json!({"op":"copy_hello","data":{"versions":[1],"codecs":["none"]}}),
        json!({"op":"open","data":{"argv":["dG9vbA==","/w==",""],"codec":"zlib"}}),
        json!({"op":"exchange","data":{"stdin":null,"stdin_eof":0,"stdout_ack":2,"stderr_ack":1}}),
        json!({"op":"cancel"}),
        json!({"op":"status"}),
    ];
    let response = [
        json!({"op":"copy_hello","data":{"version":1,"codecs":["none"],"frame_bytes":1024,"window_bytes":4096}}),
        json!({"op":"copy_refused","data":{"code":"busy","prior_binding":binding}}),
        json!({"op":"hello","data":{"codecs":["none","zlib"],"frame_bytes":1024,"window_bytes":4096}}),
        json!({"op":"opened","data":{"codec":"zlib","open_sha256":"50".repeat(32),"disposition":"existing"}}),
        json!({"op":"exchange","data":{"stdin_accepted":4,"stdin_delivered":4,"stdin_closed":true,
              "stdout":null,"stderr":null,"stdout_eof":0,"stderr_eof":0,"cancel_requested":false,
              "terminal":{"kind":"exited","data":{"code":0}}}}),
        json!({"op":"error","data":{"code":"not_found"}}),
    ];
    let terminal = [
        json!({"kind":"exited","data":{"code":254}}),
        json!({"kind":"signaled","data":{"signal":9}}),
        json!({"kind":"cancelled"}),
        json!({"kind":"failed","data":{"code":"closed"}}),
    ];
    let mut shape_pairs = pairs::<wire::Binding, projected::Binding>(&[binding.clone()]);
    shape_pairs += pairs::<wire::RequestBody, projected::RequestBody>(&request);
    shape_pairs += pairs::<wire::ResponseBody, projected::ResponseBody>(&response);
    shape_pairs += pairs::<wire::Terminal, projected::Terminal>(&terminal);

    let mut envelope_pairs = 0;
    for body in &request {
        let value = wire::Request {
            binding: serde_json::from_value(binding.clone()).unwrap(),
            sequence: u64::MAX,
            body: serde_json::from_value(body.clone()).unwrap(),
        };
        let original = wire::encode_request(&value).unwrap();
        let reordered = serde_json::from_slice::<projected::Envelope>(&original).unwrap();
        let projected = serde_json::to_vec(&reordered).unwrap();
        assert_eq!(projected, original);
        assert!(wire::decode_request(&projected).unwrap() == value);
        envelope_pairs += 1;
    }
    for body in &response {
        let value = wire::Response {
            binding: serde_json::from_value(binding.clone()).unwrap(),
            sequence: 17,
            request_sha256: [0x59; 32],
            body: serde_json::from_value(body.clone()).unwrap(),
        };
        let original = wire::encode_response(&value).unwrap();
        let projected =
            serde_json::to_vec(&serde_json::from_slice::<projected::Envelope>(&original).unwrap())
                .unwrap();
        assert_eq!(projected, original);
        assert!(wire::decode_response(&projected).unwrap() == value);
        envelope_pairs += 1;
    }

    let closed = [
        r#"{"op":"missing"}"#,
        r#"{"op":"cancel","op":"status"}"#,
        r#"{"op":"open","data":{"argv":["YQ=="],"argv":["Yg=="],"codec":"none"}}"#,
        r#"{"op":"open","data":{"argv":["YQ=="],"codec":"none","extra":0}}"#,
    ];
    for wire in closed {
        rejected::<wire::RequestBody, projected::RequestBody>(wire, None);
    }
    let malformed = [
        (vec!["!".to_owned(); 129], "argv_limit"),
        (vec!["/w".into()], "invalid stream input: base64"),
        (vec!["/x==".into()], "invalid stream input: base64"),
        (vec!["!".repeat(21849)], "stream capacity reached"),
        (vec!["AAAA".repeat(5462)], "invalid stream input: base64"),
        (vec!["AAAA".repeat(5461), "AAAA".into()], "argv_limit"),
    ];
    for (argv, diagnostic) in malformed {
        let value = json!({"op":"open","data":{"argv":argv,"codec":"none"}});
        rejected::<wire::RequestBody, projected::RequestBody>(&value.to_string(), Some(diagnostic));
    }
    let bad_binding = binding
        .to_string()
        .replace(&"30".repeat(32), &"AB".repeat(32));
    rejected::<wire::Binding, projected::Binding>(&bad_binding, Some("hex"));

    // Structural admission alone permits an empty argv. The original wire
    // operation applies a separate guard.
    let empty = json!({"op":"open","data":{"argv":[],"codec":"none"}});
    assert!(serde_json::from_value::<projected::RequestBody>(empty.clone()).is_ok());
    let candidate = wire::Request {
        binding: serde_json::from_value(binding).unwrap(),
        sequence: 0,
        body: serde_json::from_value(empty).unwrap(),
    };
    assert!(matches!(
        wire::encode_request(&candidate),
        Err(marmish_stream::Error::Limit)
    ));
    println!(
        "{}",
        json!({
            "matchedPublicShapePairs":shape_pairs,
            "matchedCanonicalOriginalEnvelopePairs":envelope_pairs,
            "matchedStructuralControls":closed.len() + 7,
            "guardDistinctionObserved":true,
            "externalBindings":"original-public-types",
        })
    );
}
