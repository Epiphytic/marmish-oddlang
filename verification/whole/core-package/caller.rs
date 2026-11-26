use marmish_core::{ConfigError, FrameError, Limits, ProbeEnvelope, RelaySet, Retention};
use serde_json::{json, Value};
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

struct Transcript(BufWriter<File>);

impl Transcript {
    fn observe(&mut self, family: &str, case: &str, outcome: Value) {
        serde_json::to_writer(&mut self.0, &json!([family, case, outcome])).unwrap();
        self.0.write_all(b"\n").unwrap();
    }

    fn relays(&mut self, case: &str, entries: &[&str], allow_loopback: bool) {
        let entries = entries
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect::<Vec<_>>();
        let outcome = match RelaySet::parse(&entries, allow_loopback) {
            Ok(set) => json!({"urls": set.as_slice()}),
            Err(error) => config_error(error),
        };
        self.observe("relay", case, outcome);
    }

    fn decode(&mut self, case: &str, bytes: &[u8]) {
        self.frame("decode", case, ProbeEnvelope::decode(bytes));
    }

    fn frame(&mut self, family: &str, case: &str, result: Result<Vec<u8>, FrameError>) {
        let outcome = match result {
            Ok(bytes) => json!({"bytes": bytes}),
            Err(error) => frame_error(error),
        };
        self.observe(family, case, outcome);
    }
}

fn config_error(error: ConfigError) -> Value {
    // This exhaustive match also catches a missing or additional public variant.
    let variant = match &error {
        ConfigError::EmptyRelaySet => "EmptyRelaySet",
        ConfigError::InvalidUrl => "InvalidUrl",
        ConfigError::DisallowedScheme => "DisallowedScheme",
        ConfigError::DisallowedComponents => "DisallowedComponents",
        ConfigError::DisallowedHost => "DisallowedHost",
        ConfigError::LoopbackDenied => "LoopbackDenied",
        ConfigError::RetentionOutOfRange => "RetentionOutOfRange",
    };
    let display = error.to_string();
    let debug = format!("{error:?}");
    let has_source = error.source().is_some();
    json!({"error": variant, "ordinal": error as u8, "display": display, "debug": debug,
           "hasSource": has_source})
}

fn frame_error(error: FrameError) -> Value {
    let variant = match &error {
        FrameError::SizeLimit => "SizeLimit",
        FrameError::InvalidJson => "InvalidJson",
        FrameError::InvalidVersion => "InvalidVersion",
        FrameError::InvalidEncoding => "InvalidEncoding",
    };
    let display = error.to_string();
    let debug = format!("{error:?}");
    let has_source = error.source().is_some();
    json!({"error": variant, "ordinal": error as u8, "display": display, "debug": debug,
           "hasSource": has_source})
}

// Fixed pseudorandom sequence, independent of the core or any external random provider.
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn relay_cases(output: &mut Transcript) {
    output.relays("empty", &[], false);
    for (case, input) in [
        ("empty-string", ""),
        ("relative", "//sample.example"),
        ("scheme", "https://sample.example"),
        ("raw-at-before-scheme", "https://user@sample.example"),
        (
            "raw-whitespace-before-at",
            "https://user@sample.example\u{a0}",
        ),
        ("raw-control-before-parse", "wss://sample.example\u{7f}"),
        ("raw-comma", "wss://one.example,wss://two.example"),
        ("raw-backslash", "wss://sample.example\\"),
        ("encoded-comma", "wss://one%2ctwo.example"),
        ("raw-at-placeholder", "wss://@sample.example"),
        ("empty-query", "wss://sample.example?"),
        ("empty-fragment", "wss://sample.example#"),
        ("path", "wss://sample.example/path"),
        ("resolved-root-path", "wss://sample.example/path/.."),
        ("malformed-port", "wss://sample.example:65536/"),
        ("explicit-port", "wss://sample.example:65535/"),
        ("bad-label", "wss://bad_label.example"),
        ("leading-hyphen", "wss://-bad.example"),
        ("trailing-hyphen", "wss://bad-.example"),
        ("repeated-dot", "wss://sample.example.."),
        ("loopback-label", "ws://A.LOCALHOST."),
        ("not-loopback-label", "ws://A-NOTLOCALHOST."),
        ("short-ipv4-loopback", "ws://127.1"),
        ("hex-ipv4-loopback", "ws://0x7f000001"),
        ("ipv6-loopback", "ws://[::1]"),
        ("ipv4-mapped-loopback", "ws://[::ffff:127.0.0.1]"),
        ("ipv4-private-with-opt-in", "ws://10.0.0.1"),
        ("ipv6-deny-within-allow", "wss://[2001:db8::1]"),
        ("ipv6-allow", "wss://[2606:4700::1111]"),
    ] {
        output.relays(case, &[input], false);
        output.relays(&format!("{case}-opt-in"), &[input], true);
    }
    output.relays(
        "stable-dedup",
        &[
            "WSS://FIRST.EXAMPLE.:443",
            "wss://xn--bcher-kva.example",
            "wss://first.example",
            "wss://BÜCHER.example/",
            "ws://third.example:80",
        ],
        false,
    );
    output.relays("first-error-scheme", &["https://one.example", "bad"], false);
    output.relays("first-error-parse", &["bad", "https://one.example"], false);

    for (index, host) in [
        "0.255.255.255",
        "1.0.0.0",
        "9.255.255.255",
        "10.0.0.0",
        "10.255.255.255",
        "11.0.0.0",
        "100.63.255.255",
        "100.64.0.0",
        "100.127.255.255",
        "100.128.0.0",
        "126.255.255.255",
        "127.0.0.0",
        "127.255.255.255",
        "128.0.0.0",
        "169.253.255.255",
        "169.254.0.0",
        "169.254.255.255",
        "169.255.0.0",
        "172.15.255.255",
        "172.16.0.0",
        "172.31.255.255",
        "172.32.0.0",
        "191.255.255.255",
        "192.0.0.0",
        "192.0.0.255",
        "192.0.1.0",
        "192.0.2.0",
        "192.0.2.255",
        "192.0.3.0",
        "192.167.255.255",
        "192.168.0.0",
        "192.168.255.255",
        "192.169.0.0",
        "198.17.255.255",
        "198.18.0.0",
        "198.19.255.255",
        "198.20.0.0",
        "198.51.99.255",
        "198.51.100.0",
        "198.51.100.255",
        "198.51.101.0",
        "203.0.112.255",
        "203.0.113.0",
        "203.0.113.255",
        "203.0.114.0",
        "223.255.255.255",
        "224.0.0.0",
        "255.255.255.255",
        "[::]",
        "[::1]",
        "[::ffff:8.8.8.8]",
        "[1fff:ffff::]",
        "[2000::]",
        "[2001::]",
        "[2001:1ff:ffff::]",
        "[2001:200::]",
        "[2001:db7:ffff::]",
        "[2001:db8::]",
        "[2001:db9::]",
        "[2002::]",
        "[2003::]",
        "[3ffe:ffff::]",
        "[3fff::]",
        "[3fff:fff:ffff::]",
        "[3fff:1000::]",
        "[3fff:ffff::]",
        "[4000::]",
        "[fc00::]",
        "[fe80::]",
        "[ff00::]",
    ]
    .into_iter()
    .enumerate()
    {
        output.relays(
            &format!("prefix-{index}"),
            &[&format!("wss://{host}")],
            false,
        );
    }
}

fn frame_cases(output: &mut Transcript) {
    for error in [
        FrameError::SizeLimit,
        FrameError::InvalidJson,
        FrameError::InvalidVersion,
        FrameError::InvalidEncoding,
    ] {
        let name = format!("{error:?}");
        output.observe("frame-error", &name, frame_error(error));
    }
    output.observe(
        "limits",
        "public",
        json!([
            Limits::RAW_BYTES,
            Limits::JSON_BYTES,
            Limits::NDJSON_BYTES,
            format!("{:?}", Limits)
        ]),
    );
    for (case, input) in [
        ("empty", ""),
        ("object-empty", "{}"),
        ("tuple-empty", "[]"),
        ("tuple-short", "[1]"),
        ("tuple-good", r#"[1,"AAH/"]"#),
        ("tuple-long", r#"[1,"AAH/",null]"#),
        ("tuple-wrong-positions", r#"["AAH/",1]"#),
        ("tuple-escaped", r#"[1,"\u005a\u0067=="]"#),
        ("tuple-version-before-encoding", r#"[2,"!!"]"#),
        ("object-good", r#"{"version":1,"payload":"AAH/"}"#),
        ("object-reordered", r#"{"payload":"AAH/","version":1}"#),
        (
            "object-duplicate",
            r#"{"version":1,"payload":"AAH/","payload":""}"#,
        ),
        (
            "object-unknown",
            r#"{"version":1,"payload":"","more":null}"#,
        ),
        ("json-before-version", r#"{"version":2,"payload":false}"#),
        ("version-before-encoding", r#"{"version":2,"payload":"!"}"#),
        ("version-overflow", r#"[4294967296,""]"#),
        ("version-max", r#"[4294967295,""]"#),
        ("version-negative", r#"[-1,""]"#),
        ("version-float", r#"[1.0,""]"#),
        ("version-exponent", r#"[1e0,""]"#),
        ("payload-padding", r#"[1,"Zg=="]"#),
        ("payload-missing-padding", r#"[1,"Zg"]"#),
        ("payload-nonzero-tail", r#"[1,"Zh=="]"#),
        ("payload-overpadding", r#"[1,"Zg==="]"#),
        ("payload-linebreak", r#"[1,"Zg==\n"]"#),
        ("payload-url-safe", r#"[1,"_-__"]"#),
        ("payload-unpaired-surrogate", r#"[1,"\ud800"]"#),
        ("trailing-whitespace", "[1, \"\"] \r\n"),
        ("trailing-value", r#"[1,""] 1"#),
    ] {
        output.decode(case, input.as_bytes());
    }
    for size in [
        Limits::RAW_BYTES - 2,
        Limits::RAW_BYTES - 1,
        Limits::RAW_BYTES,
        Limits::RAW_BYTES + 1,
    ] {
        let bytes = (0..size).map(|i| i as u8).collect::<Vec<_>>();
        let encoded = ProbeEnvelope::encode(&bytes);
        if let Ok(frame) = &encoded {
            output.decode(&format!("roundtrip-{size}"), frame);
        }
        output.frame("encode", &format!("raw-{size}"), encoded);
    }
    let wire = br#"{"version":1,"payload":""}"#;
    for size in [
        Limits::JSON_BYTES - 1,
        Limits::JSON_BYTES,
        Limits::JSON_BYTES + 1,
    ] {
        let mut padded = wire.to_vec();
        padded.resize(size, b' ');
        output.decode(&format!("wire-{size}"), &padded);
        output.decode(&format!("bad-wire-{size}"), &vec![b'!'; size]);
    }
    let encoded_limit = Limits::RAW_BYTES.div_ceil(3) * 4;
    for size in [encoded_limit - 1, encoded_limit, encoded_limit + 1] {
        for version in [1, 2] {
            let encoded = json!([version, "A".repeat(size)]).to_string();
            output.decode(&format!("encoded-{version}-{size}"), encoded.as_bytes());
            let invalid = json!([version, "!".repeat(size)]).to_string();
            output.decode(
                &format!("malformed-encoded-{version}-{size}"),
                invalid.as_bytes(),
            );
        }
    }
    let mut seed = 0xf0a5_48dc_8fb4_f3dd;
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=_-!\n";
    for i in 0..512 {
        let size = (next(&mut seed) % 96) as usize;
        let raw = (0..size).map(|_| next(&mut seed) as u8).collect::<Vec<_>>();
        output.decode(&format!("arbitrary-{i}"), &raw);
        output.frame("encode", &format!("seed-{i}"), ProbeEnvelope::encode(&raw));
        let payload = (0..size)
            .map(|_| alphabet[next(&mut seed) as usize % alphabet.len()] as char)
            .collect::<String>();
        let version = [1, 1, 2, 0, u32::MAX][next(&mut seed) as usize % 5];
        let frame = if next(&mut seed) & 1 == 0 {
            json!([version, payload])
        } else {
            json!({"version":version,"payload":payload})
        }
        .to_string();
        output.decode(&format!("grammar-{i}"), frame.as_bytes());
    }
}

#[test]
fn differential() {
    let path = std::env::var_os("ODDLANG_CORE_TRANSCRIPT").expect("transcript path");
    let mut output = Transcript(BufWriter::new(File::create(path).unwrap()));
    for seconds in [
        None,
        Some(0),
        Some(1),
        Some(31_535_999),
        Some(31_536_000),
        Some(31_536_001),
        Some(u64::MAX),
    ] {
        let outcome = match Retention::from_seconds(seconds) {
            Ok(retention) => json!({"seconds": retention.seconds()}),
            Err(error) => config_error(error),
        };
        output.observe("retention", &format!("{seconds:?}"), outcome);
    }
    output.observe(
        "retention",
        "defaults",
        json!([Retention::default().seconds(), Retention::MAX_SECONDS]),
    );
    relay_cases(&mut output);
    frame_cases(&mut output);
    output.0.flush().unwrap();
}
