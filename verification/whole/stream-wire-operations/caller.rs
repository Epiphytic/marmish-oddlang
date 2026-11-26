// Built unchanged against each peer. Record bytes and named errors on stdout.
use marmish_stream::{
    Error,
    codec::{self, Codec, EncodedBlock},
    copy_protocol::Launch,
    wire::{
        self, Binding, CopyAdmissionError, OpenDisposition, Request, RequestBody, Response,
        ResponseBody, Terminal, WireErrorCode,
    },
};
use serde_json::json;

fn error(e: Error) -> String {
    format!("{e:?} | {e}")
}

fn shown(bytes: Result<Vec<u8>, Error>) -> String {
    bytes
        .map(|value| String::from_utf8(value).unwrap())
        .unwrap_or_else(error)
}

fn entry(name: &str, fields: serde_json::Value) {
    println!("{}", json!({"case": name, "outcomes": fields}));
}

fn binding(timestamp: u64) -> Binding {
    let mut stream = [7; 16];
    stream[..8].copy_from_slice(&timestamp.to_be_bytes());
    Binding {
        stream,
        session: [8; 16],
        client: [9; 32],
        server: [10; 32],
    }
}

fn request(name: &str, stream: Binding, body: RequestBody, now: u64) {
    let input = Request {
        binding: stream,
        sequence: 14,
        body,
    };
    let encoded = wire::encode_request(&input);
    let mut result = json!({
        "encode": shown(encoded.clone()),
        "at": input.validate_at(now).map_err(error),
    });
    if let Ok(bytes) = encoded {
        result["decode"] = json!(
            wire::decode_request(&bytes)
                .map(|v| v == input)
                .map_err(error)
        );
        result["otherDirection"] = json!(wire::decode_response(&bytes).err().map(error));
    }
    entry(name, result);
}

fn response(name: &str, body: ResponseBody) {
    let input = Response {
        binding: binding(1000),
        sequence: 15,
        request_sha256: [11; 32],
        body,
    };
    let encoded = wire::encode_response(&input);
    let mut result = json!({"encode": shown(encoded.clone())});
    if let Ok(bytes) = encoded {
        result["decode"] = json!(
            wire::decode_response(&bytes)
                .map(|v| v == input)
                .map_err(error)
        );
        result["otherDirection"] = json!(wire::decode_request(&bytes).err().map(error));
    }
    entry(name, result);
}

fn launch(version: u32, client: u8, source: &str, destination: &str) -> Launch {
    serde_json::from_value(json!({
        "version": version, "mode": "create", "transfer": "11".repeat(32),
        "client": format!("{client:02x}").repeat(32), "server": "0a".repeat(32),
        "direction":"put", "source":source, "destination":destination,
        "overwrite": false, "preserve_mode": true, "preserve_mtime": false,
    }))
    .unwrap()
}

fn input(
    name: &str,
    block: Option<EncodedBlock>,
    eof: Option<u64>,
    stdout_ack: u64,
    stderr_ack: u64,
) {
    request(
        name,
        binding(1000),
        RequestBody::Exchange {
            stdin: block,
            stdin_eof: eof,
            stdout_ack,
            stderr_ack,
        },
        1000,
    );
}

fn output(
    name: &str,
    status: (u64, u64, bool, bool),
    stdout: (Option<EncodedBlock>, Option<u64>),
    stderr: (Option<EncodedBlock>, Option<u64>),
    terminal: Option<Terminal>,
) {
    let (stdin_accepted, stdin_delivered, stdin_closed, cancel_requested) = status;
    let (stdout, stdout_eof) = stdout;
    let (stderr, stderr_eof) = stderr;
    response(
        name,
        ResponseBody::Exchange {
            stdin_accepted,
            stdin_delivered,
            stdin_closed,
            stdout,
            stderr,
            stdout_eof,
            stderr_eof,
            cancel_requested,
            terminal,
        },
    );
}

fn main() {
    use Codec::{None as Plain, Zlib};
    use RequestBody::*;
    assert_eq!((wire::PROTOCOL, wire::FORMAT), ("marmish-stream/1", 1));
    let bound = binding(1000);
    for (name, codecs) in [
        ("hello", vec![Plain, Zlib]),
        ("hello-empty", vec![]),
        ("hello-duplicate", vec![Zlib, Zlib]),
        ("hello-overflow", vec![Plain, Zlib, Plain]),
    ] {
        request(name, bound.clone(), Hello { codecs }, 1000);
    }
    for (name, versions) in [
        ("copy-hello", vec![1, 2, 3, 4]),
        ("copy-hello-empty", vec![]),
        ("copy-hello-repeat", vec![1, 2, 1]),
        ("copy-hello-zero", vec![1, 0]),
        ("copy-hello-limit", vec![1, 2, 3, 4, 5]),
    ] {
        request(
            name,
            bound.clone(),
            CopyHello {
                versions,
                codecs: vec![Plain],
            },
            1000,
        );
    }
    for (name, version, client, source, destination) in [
        ("open-copy", 1, 9, "c291cmNl", "ZGVzdA=="),
        ("open-copy-account", 1, 8, "c291cmNl", "ZGVzdA=="),
        (
            "open-copy-version-before-account",
            2,
            8,
            "c291cmNl",
            "ZGVzdA==",
        ),
        ("open-copy-source-before-account", 1, 8, "", "ZGVzdA=="),
        ("open-copy-invalid-destination", 1, 9, "c291cmNl", "AA=="),
    ] {
        request(
            name,
            bound.clone(),
            OpenCopy {
                launch: launch(version, client, source, destination),
                codec: Zlib,
            },
            1000,
        );
    }
    for (name, length, now) in [
        ("open-min-age", 1, 940),
        ("open-future", 1, 939),
        ("open-max-age", 1, 605800),
        ("open-expired", 1, 605801),
        ("open-largest-arg", 16383, 1000),
        ("open-arg-overflow", 16384, 939),
    ] {
        request(
            name,
            bound.clone(),
            Open {
                argv: vec![vec![b'x'; length]],
                codec: Plain,
            },
            now,
        );
    }
    for (name, argv) in [
        ("open-empty-argv", vec![]),
        ("open-empty-name", vec![vec![]]),
        ("open-nul-arg", vec![vec![b'x'], vec![0]]),
        ("open-argc", vec![vec![b'x']; 128]),
        ("open-argc-overflow", vec![vec![b'x']; 129]),
        ("open-empty-nonfirst", vec![vec![b'x'], vec![]]),
    ] {
        let digest = wire::open_digest(&argv, Plain).map_err(error);
        entry(&format!("{name}-digest"), json!(digest));
        request(name, bound.clone(), Open { argv, codec: Plain }, 1000);
    }
    request(
        "open-saturating-future",
        binding(u64::MAX),
        Open {
            argv: vec![b"x".to_vec()],
            codec: Plain,
        },
        u64::MAX - 1,
    );
    request("cancel", bound.clone(), Cancel, 0);
    request("status", bound.clone(), Status, u64::MAX);

    let valid = codec::encode(7, b"abc", Plain).unwrap();
    let mut invalid = valid.clone();
    invalid.raw_len = 0;
    let over = 1u64 << 63;
    input("input-block", Some(valid.clone()), Some(10), 0, over - 1);
    input(
        "input-block-before-eof",
        Some(invalid.clone()),
        Some(9),
        0,
        0,
    );
    input(
        "input-ack-before-block",
        Some(invalid.clone()),
        Some(9),
        over,
        0,
    );
    input("input-stderr-ack", Some(invalid.clone()), Some(9), 0, over);
    input(
        "input-eof-before-block",
        Some(invalid.clone()),
        Some(over),
        0,
        0,
    );
    input("input-eof-mismatch", Some(valid.clone()), Some(11), 0, 0);
    input("input-none-eof", None, Some(10), 0, 0);

    for (name, version, codecs, frame_bytes, window_bytes) in [
        ("reply-copy-hello", 1, vec![Plain], 8192, 262144),
        ("reply-copy-wrong-version", 2, vec![Plain], 8192, 262144),
        ("reply-copy-version-before-limit", 2, vec![Plain], 0, 0),
        ("reply-codecs-before-limit", 1, vec![], 0, 0),
        ("reply-frame-over-limit", 1, vec![Plain], 8193, 262144),
        ("reply-window-over-limit", 1, vec![Plain], 1, 262145),
    ] {
        response(
            name,
            ResponseBody::CopyHello {
                version,
                codecs,
                frame_bytes,
                window_bytes,
            },
        );
    }
    response(
        "reply-hello",
        ResponseBody::Hello {
            codecs: vec![Zlib],
            frame_bytes: 1,
            window_bytes: 1,
        },
    );
    response(
        "reply-window-under-frame",
        ResponseBody::Hello {
            codecs: vec![Plain],
            frame_bytes: 2,
            window_bytes: 1,
        },
    );
    response(
        "response-opened",
        ResponseBody::Opened {
            codec: Plain,
            open_sha256: [12; 32],
            disposition: OpenDisposition::Existing,
        },
    );
    response(
        "response-error",
        ResponseBody::Error {
            code: WireErrorCode::Denied,
        },
    );
    for (name, code, prior_binding) in [
        (
            "refused-valid",
            CopyAdmissionError::Busy,
            Some(bound.clone()),
        ),
        ("refused-missing-prior", CopyAdmissionError::Denied, None),
        (
            "refused-invalid-code",
            CopyAdmissionError::Denied,
            Some(bound.clone()),
        ),
        (
            "refused-invalid-account",
            CopyAdmissionError::RecoveryRequired,
            Some(Binding {
                client: [1; 32],
                ..bound.clone()
            }),
        ),
    ] {
        response(
            name,
            ResponseBody::CopyRefused {
                code,
                prior_binding,
            },
        );
    }
    let empty = (None, None);
    output(
        "output-offset-before-ack",
        (over, over - 1, false, false),
        empty.clone(),
        empty.clone(),
        None,
    );
    output(
        "output-ack-before-closed",
        (1, 2, false, true),
        (Some(invalid.clone()), None),
        empty.clone(),
        None,
    );
    output(
        "output-closed-before-block",
        (2, 1, false, true),
        (Some(invalid.clone()), None),
        empty.clone(),
        None,
    );
    output(
        "output-stdout-eof-before-block",
        (2, 1, true, false),
        (Some(invalid.clone()), Some(over)),
        empty.clone(),
        None,
    );
    output(
        "output-stdout-before-stderr",
        (2, 1, true, false),
        (Some(invalid.clone()), None),
        (None, Some(over)),
        None,
    );
    output(
        "output-stderr-beyond-eof",
        (2, 1, true, false),
        empty.clone(),
        (Some(valid.clone()), Some(9)),
        None,
    );
    output(
        "output-terminal-needs-eof",
        (2, 1, true, false),
        empty.clone(),
        empty.clone(),
        Some(Terminal::Signaled { signal: 0 }),
    );
    output(
        "output-invalid-signal",
        (2, 1, true, false),
        (None, Some(11)),
        (None, Some(9)),
        Some(Terminal::Signaled { signal: 0 }),
    );
    output(
        "output-unsolicited-cancel",
        (2, 1, true, false),
        (None, Some(11)),
        (None, Some(9)),
        Some(Terminal::Cancelled),
    );
    output(
        "output-cancelled",
        (2, 1, true, true),
        (None, Some(11)),
        (None, Some(9)),
        Some(Terminal::Cancelled),
    );
    output(
        "output-success",
        (2, 1, true, false),
        (Some(valid.clone()), Some(10)),
        (Some(valid), Some(11)),
        Some(Terminal::Exited { code: 0 }),
    );

    let sample = wire::encode_request(&Request {
        binding: bound,
        sequence: 3,
        body: Exchange {
            stdin: None,
            stdin_eof: None,
            stdout_ack: 0,
            stderr_ack: 0,
        },
    })
    .unwrap();
    let sample = String::from_utf8(sample).unwrap();
    for (name, source) in [
        ("canonical", sample.clone()),
        ("canonical-whitespace", format!("{sample}\n")),
        ("canonical-limit", " ".repeat(32769)),
        ("canonical-exact-limit", " ".repeat(32768)),
        (
            "canonical-missing-option",
            sample.replace("\"stdin\":null,", ""),
        ),
        (
            "canonical-duplicate",
            sample.replace("\"sequence\":3", "\"sequence\":3,\"sequence\":3"),
        ),
        (
            "canonical-unexpected",
            sample.replace("\"sequence\":3", "\"sequence\":3,\"unknown\":0"),
        ),
        (
            "canonical-swapped-fields",
            sample.replace(
                "\"stdin\":null,\"stdin_eof\":null",
                "\"stdin_eof\":null,\"stdin\":null",
            ),
        ),
        (
            "canonical-version",
            sample.replace("\"version\":1", "\"version\":9"),
        ),
        (
            "canonical-protocol",
            sample.replace("marmish-stream/1", "another/1"),
        ),
        (
            "canonical-unsigned-syntax",
            sample.replace("\"sequence\":3", "\"sequence\":3.0"),
        ),
        (
            "canonical-hex-uppercase",
            sample.replace(&"0a".repeat(32), &"0A".repeat(32)),
        ),
        (
            "canonical-direct-guard",
            sample.replace("\"stdout_ack\":0", "\"stdout_ack\":9223372036854775808"),
        ),
        (
            "canonical-error-before-guard",
            sample.replace("\"stdout_ack\":0", "\"stdout_ack\": 9223372036854775808"),
        ),
    ] {
        let raw = source.as_bytes();
        entry(
            name,
            json!({"request":wire::decode_request(raw).map(|_| ()).map_err(error),
                          "response":wire::decode_response(raw).map(|_| ()).map_err(error),
                          "digest":wire::request_digest(raw)}),
        );
    }
}
