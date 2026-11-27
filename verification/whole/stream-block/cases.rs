use base64::{Engine, engine::general_purpose::STANDARD};
use public_data_qualification::{Result, assembly::bytes_sha, capture, list, require, text};
use serde_json::{Value, json};
use std::{io::Write, path::Path, process::Command};
use stream_wire_replay::native::Random;
const LIMIT: usize = 8192;
const MAX: u64 = i64::MAX as u64;
pub fn send(exe: &Path, cases: &Value) -> Result<Value> {
    let cases = cases.as_object().ok_or("expected named cases")?;
    let mut input = Vec::new();
    for (id, request) in cases {
        serde_json::to_writer(&mut input, &(id, request))?;
        writeln!(input)?;
    }
    let output = capture(&mut Command::new(exe), 600, Some(&input))?;
    require(
        output.status.success(),
        String::from_utf8_lossy(&output.stderr),
    )?;
    let rows: Vec<(String, Value)> = std::str::from_utf8(&output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.iter().map(|(name, _)| name).eq(cases.keys()),
        "peer omitted, reordered or duplicated a result",
    )?;
    Ok(Value::Object(rows.into_iter().collect()))
}
fn encode_cases() -> Value {
    let mut rng = Random::new(0x54fe);
    let mut cases = json!({});
    for size in [
        0,
        1,
        2,
        3,
        4,
        17,
        31,
        127,
        1024,
        LIMIT - 1,
        LIMIT,
        LIMIT + 1,
    ] {
        for (family, bytes) in [("run", vec![129; size]), ("noise", rng.bytes(size))] {
            for label in ["none", "zlib"] {
                for offset in [0, MAX - size as u64, MAX - size as u64 + 1, u64::MAX] {
                    cases[format!("{family}/{size}/{label}/{offset}")] = json!({"operation":"encode","offset":offset,"bytes":bytes,"negotiated":label});
                }
            }
        }
    }
    cases
}
fn decode_cases() -> Result<Value> {
    let raw = b"blue".repeat(173);
    // Frozen independently produced Python/zlib member for this payload.
    let member = STANDARD.decode("eJxLyilNTRrFo3gIYQDmYB6Y")?;
    let wire = |data: &[u8]| json!({"offset":7,"codec":"zlib","raw_len":raw.len(),"sha256":bytes_sha(&raw),"data":STANDARD.encode(data)});
    let base = wire(&member);
    let mut witnesses = json!({});
    witnesses["valid"] = base.clone();
    witnesses["concatenated"] = wire(&member.repeat(2));
    let mut trailing = member.clone();
    trailing.push(0);
    witnesses["trailing"] = wire(&trailing);
    let mut dictionary = member.clone();
    dictionary[1] |= 32;
    witnesses["dictionary"] = wire(&dictionary);
    for n in 0..member.len() {
        witnesses[format!("truncated/{n}")] = wire(&member[..n]);
    }
    for (k, v) in [
        ("codec", json!("none")),
        ("raw_len", json!(0)),
        ("raw_len", json!(member.len())),
        ("raw_len", json!(8193)),
        ("offset", json!(MAX)),
        ("sha256", json!("00".repeat(32))),
        ("sha256", json!("AA".repeat(32))),
        ("sha256", json!("")),
        ("data", json!("Zh==")),
        ("data", json!("A".repeat(10925))),
        ("extra", json!(1)),
    ] {
        let label = v
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string());
        let mut w = base.clone();
        w[k] = v;
        witnesses[format!("field/{k}/{}", label.chars().take(8).collect::<String>())] = w;
    }
    let tuple: Vec<_> = base.as_object().unwrap().values().cloned().collect();
    witnesses["tuple"] = json!(tuple);
    let mut extra = tuple.clone();
    extra.push(json!(0));
    witnesses["tuple-extra"] = json!(extra);
    witnesses["tuple-short"] = json!(&tuple[..tuple.len() - 1]);
    for k in base.as_object().unwrap().keys() {
        let mut w = base.clone();
        w.as_object_mut().unwrap().shift_remove(k);
        witnesses[format!("missing/{k}")] = w;
    }
    let mut cases = json!({});
    for (k, w) in witnesses.as_object().unwrap() {
        for label in ["none", "zlib"] {
            cases[format!("{k}/{label}")] =
                json!({"operation":"decode","wire":w,"negotiated":label});
        }
    }
    Ok(cases)
}

#[link(name = "z")]
unsafe extern "C" {
    fn uncompress(
        dst: *mut u8,
        len: *mut std::ffi::c_ulong,
        src: *const u8,
        src_len: std::ffi::c_ulong,
    ) -> std::ffi::c_int;
}
fn independently_decodes(input: &[u8], expected: &[u8]) -> bool {
    let mut out = vec![0; expected.len()];
    let mut len = out.len() as std::ffi::c_ulong;
    // zlib receives valid slice bounds and reports the number of initialized output bytes.
    let status = unsafe {
        uncompress(
            out.as_mut_ptr(),
            &mut len,
            input.as_ptr(),
            input.len() as std::ffi::c_ulong,
        )
    };
    status == 0 && len as usize == expected.len() && out == expected
}
pub fn qualify(reference: &Path, candidate: &Path) -> Result<Value> {
    let declarations = json!({"public":{"operation":"declarations","negotiated":"none"}});
    let public = send(reference, &declarations)?;
    require(
        public == send(candidate, &declarations)?
            && public["public"]["limits"] == json!([8192, 8192, 32768, 262144, MAX, 256]),
        "public declarations differ",
    )?;
    let samples = list(&public["public"]["formattedInvalid"])?;
    require(
        samples
            .iter()
            .map(|r| r["payload"].as_str())
            .eq(["", "sentinel", "naïve{input}"].map(Some)),
        "wrong invalid display cases",
    )?;
    for r in samples {
        let expected = format!("invalid stream input: {}", text(&r["payload"])?);
        for fmt in [
            "default",
            "rightWidth",
            "precision",
            "centerFill",
            "leftWidthAndPrecision",
        ] {
            require(r[fmt] == expected, "native Invalid display differs")?;
        }
    }
    let enc = encode_cases();
    let original = send(reference, &enc)?;
    let generated = send(candidate, &enc)?;
    let mut mixed = json!({});
    let mut expected = json!({});
    for (id, request) in enc.as_object().unwrap() {
        let raw: Vec<u8> = serde_json::from_value(request["bytes"].clone())?;
        let offset = request["offset"].as_u64().ok_or("invalid offset")?;
        let error = if !(1..=LIMIT).contains(&raw.len()) {
            Some("Limit")
        } else if offset > MAX - raw.len() as u64 {
            Some("Offset")
        } else {
            None
        };
        if let Some(error) = error {
            require(
                original[id] == generated[id] && original[id]["error"] == error,
                id,
            )?;
            continue;
        }
        for (label, r) in [("original", &original[id]), ("generated", &generated[id])] {
            let w = &r["wire"];
            require(
                w["offset"] == offset
                    && w["raw_len"] == raw.len()
                    && w["sha256"] == bytes_sha(&raw),
                id,
            )?;
            let data = STANDARD.decode(text(&w["data"])?)?;
            if w["codec"] == "none" {
                require(data == raw, id)?
            } else {
                require(
                    w["codec"] == "zlib"
                        && request["negotiated"] == "zlib"
                        && data.len() < raw.len()
                        && independently_decodes(&data, &raw),
                    id,
                )?;
            }
            let key = format!("{id}/{label}");
            mixed[&key] = json!({"operation":"decode","wire":w,"negotiated":request["negotiated"]});
            expected[key] = json!({"decoded":{"Ok":raw},"valid":{"Ok":null},"end":{"Ok":offset+raw.len() as u64}});
        }
        require(
            original[id]["wire"]["codec"] == generated[id]["wire"]["codec"],
            id,
        )?;
    }
    for exe in [reference, candidate] {
        require(
            send(exe, &mixed)? == expected,
            "mixed compressed stream observations differ",
        )?;
    }
    let bad = decode_cases()?;
    require(
        send(reference, &bad)? == send(candidate, &bad)?,
        "malformed input observations differ",
    )?;
    Ok(
        json!({"publicErrorVariants":9,"publicConstants":6,"formattedInvalidCases":samples.len()*5,"formattedInvalidObservations":samples,"encodeCases":enc.as_object().unwrap().len(),"decodeCases":bad.as_object().unwrap().len(),"crossReadsOnEachPeer":mixed.as_object().unwrap().len(),"validSeedDigest":bytes_sha(&b"blue".repeat(173))}),
    )
}
#[test]
fn complete_corpus_matches_predecessor() -> Result {
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&encode_cases())?),
        "79246de712b4034cba5df547f0eef444949b56a259ed8651b87ad19b83d79b68"
    );
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&decode_cases()?)?),
        "2ca21d1842ad0b68cb4a3de7ab795c61b80077286a5253c9537c23b6fc66c8dd"
    );
    Ok(())
}
