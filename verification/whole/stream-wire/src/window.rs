use crate::native::{Random, caller};
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, bytes_sha},
    capture, list, require, run, sha, whole,
};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path, process::Command};

pub const PUBLIC: [&str; 2] = [
    "bounded_output_reclaims_only_consumed_prefix",
    "exact_retry_conflict_gap_eof_and_partial_delivery",
];
const WHITEBOX: [&str; 3] = [
    "many_windows_remain_binary_exact_and_bounded",
    "receipt_memory_and_tiny_chunk_counts_are_bounded",
    "boundary_failures_do_not_mutate_windows",
];
fn traces() -> Result<Vec<Value>> {
    let bytes = run(Command::new("gzip")
        .arg("-dc")
        .arg(whole().join("stream-windows/frozen-cases.json.gz")))?;
    let mut cases: Vec<Value> = serde_json::from_slice(&bytes)?;
    let mut rng = Random::new(0x507204);
    for role in ["send_window", "receive_window"] {
        let source = role == "send_window";
        for seed in 0..48 {
            let mut tape = Vec::new();
            let mut high = 0u64;
            for _ in 0..64 {
                let cursor = [
                    0,
                    high,
                    high + 1,
                    high.saturating_sub(1),
                    i64::MAX as u64,
                    u64::MAX,
                ][rng.below(6)];
                let size = [0, 1, 2, 3, 31, 70][rng.below(6)];
                let op = rng.below(10);
                let item = if op < 4 {
                    let hex = rng
                        .bytes(size)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    let mut item = json!({"op":if source{"push"}else{"accept"},"bytes":hex});
                    if !source {
                        item["offset"] = json!(if op.is_multiple_of(2) { high } else { cursor });
                    }
                    if size != 0 && (source || item["offset"] == high) {
                        high += size as u64;
                    }
                    item
                } else if op < 7 {
                    let mut item = json!({"op":if source{"read"}else{"pending"},"limit":([0,1,2,70,8192,8193][rng.below(6)])});
                    if source {
                        item["offset"] = json!(cursor);
                    }
                    item
                } else if op < 9 {
                    json!({"op":if source{"ack"}else{"deliver"},"offset":cursor})
                } else {
                    json!({"op":"finish","offset":cursor})
                };
                tape.push(item);
                tape.push(json!({"op":"state"}));
            }
            cases.push(
                json!({"name":format!("{role}-actions-{seed}"),"operation":role,"input":tape}),
            );
        }
    }
    Ok(cases)
}
fn observations(exe: &Path, payload: &[u8], cases: &[Value]) -> Result<Vec<Value>> {
    let output = capture(&mut Command::new(exe), 600, Some(payload))?;
    require(
        output.status.success(),
        String::from_utf8_lossy(&output.stderr),
    )?;
    let rows: Vec<Value> = std::str::from_utf8(&output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.len() == cases.len()
            && rows.iter().zip(cases).all(|(row, c)| {
                row.as_array()
                    .is_some_and(|r| r.len() == 2 && r[0] == c["name"])
            }),
        "trace count or order differs",
    )?;
    Ok(rows)
}
pub fn compare(original: &Path, generated: &Path) -> Result<Value> {
    assembly::checkout(original, ORIGINAL, false)?;
    let source = fs::read_to_string(original.join("crates/marmish-stream/src/windows.rs"))?;
    let mut public = String::from(
        "use marmish_stream::{Error, MAX_BLOCK, WINDOW_BYTES};\nuse marmish_stream::windows::*;\n",
    );
    for name in PUBLIC {
        let marker = format!("    #[test]\n    fn {name}() {{");
        let start = source
            .find(&marker)
            .ok_or_else(|| format!("missing upstream test {name}"))?;
        let tail = &source[start..];
        let end = tail.find("\n    }\n").ok_or("unterminated public test")? + 7;
        public.push_str(&tail[..end]);
    }
    let temp = tempfile::tempdir()?;
    let main = whole().join("stream-windows/caller.rs");
    let reference = caller(
        &original.join("crates/marmish-stream"),
        &temp.path().join("reference"),
        &original.join("Cargo.lock"),
        &main,
        public.as_bytes(),
        &PUBLIC,
    )?;
    let candidate = caller(
        generated,
        &temp.path().join("candidate"),
        &generated.join("Cargo.lock"),
        &main,
        public.as_bytes(),
        &PUBLIC,
    )?;
    let cases = traces()?;
    let mut payload = Vec::new();
    for c in &cases {
        serde_json::to_writer(&mut payload, c)?;
        writeln!(payload)?;
    }
    let rows = observations(&reference, &payload, &cases)?;
    let observed = observations(&candidate, &payload, &cases)?;
    for ((case, left), right) in cases.iter().zip(&rows).zip(&observed) {
        require(
            left == right && case.get("expected").is_none_or(|e| e == &left[1]),
            format!("stateful trace diverged: {}", case["name"]),
        )?;
    }
    let frozen = cases.iter().filter(|c| c.get("expected").is_some()).count();
    let mut count = 0;
    for c in &cases {
        count += list(&c["input"])?.len();
    }
    Ok(
        json!({"source":ORIGINAL,"publicUpstreamTests":PUBLIC,"privateStructuralTests":WHITEBOX,"frozenTraces":frozen,"seededTraces":cases.len()-frozen,"operationResults":count,
        "packageLockSha256":sha(&generated.join("package.lock"))?,"transcriptSha256":bytes_sha(&serde_json::to_vec(&rows)?) }),
    )
}
#[test]
fn complete_corpus_matches_predecessor() -> Result {
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&traces()?)?),
        "92629376b73004630c524cf2c609bbb3d0e2a6c644b83defa6ad167b02098539"
    );
    Ok(())
}
