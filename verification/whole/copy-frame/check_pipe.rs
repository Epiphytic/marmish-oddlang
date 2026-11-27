//! Compare real two-process exchanges with independently encoded copy Data records.
use clap::Parser;
use public_data_qualification::{
    assembly::{ORIGINAL, bytes_sha, checkout},
    product::check_members,
    *,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    generated: PathBuf,
}
const LOCK: &str = "2efe4cea5e65e5918024f7f79465c92297e47dc019a66399b4e5894eb6eb172c";

fn build(work: &Path, workspace: &Path, member: &str) -> Result<PathBuf> {
    stream_caller::build(
        work,
        &workspace.join(member),
        &workspace.join("Cargo.lock"),
        include_bytes!("pipe.rs"),
        "copy-frame-pipe",
        "2024",
    )
}

fn corpus() -> Vec<u8> {
    let len = 4 * 1024 * 1024 + 37;
    let mut bytes = Vec::with_capacity(len + 32);
    bytes.extend(0..=255u8);
    let blocks = (len - bytes.len()).div_ceil(32);
    for i in 0..blocks {
        bytes.extend(Sha256::digest((i as u64).to_be_bytes()));
    }
    bytes.truncate(len);
    bytes
}
fn expected(payload: &[u8]) -> (Vec<u8>, usize) {
    let mut wire = Vec::new();
    let mut offset = 0;
    let mut frames = 0;
    while offset < payload.len() {
        let end = payload
            .len()
            .min(offset + [1, 7, 257, 8191, 8192][frames % 5]);
        wire.push(5);
        wire.extend(((end - offset + 8) as u32).to_be_bytes());
        wire.extend((offset as u64).to_be_bytes());
        wire.extend(&payload[offset..end]);
        offset = end;
        frames += 1;
    }
    (wire, frames)
}

fn exchange(
    encoder: &Path,
    decoder: &Path,
    input: &Path,
    payload: &[u8],
    wire: &[u8],
) -> Result<Value> {
    let output = tempfile::NamedTempFile::new()?;
    let destination = output.reopen()?;
    let mut tx = Command::new(encoder)
        .arg("encode")
        .stdin(fs::File::open(input)?)
        .stdout(Stdio::piped())
        .spawn()?;
    let receiver = Command::new(decoder)
        .arg("decode")
        .stdin(Stdio::piped())
        .stdout(destination)
        .spawn();
    let mut rx = match receiver {
        Ok(child) => child,
        Err(err) => {
            drop(tx.stdout.take());
            tx.wait()?;
            return Err(err.into());
        }
    };
    let transmission = (|| -> Result<usize> {
        let mut source = tx.stdout.take().ok_or("missing encoder pipe")?;
        let mut sink = rx.stdin.take().ok_or("missing decoder pipe")?;
        let mut pos = 0;
        let mut block = [0; 16384];
        loop {
            let len = source.read(&mut block)?;
            if len == 0 {
                break;
            }
            require(
                wire.get(pos..pos + len) == Some(&block[..len]),
                format!("frame bytes differ in block starting at {pos}"),
            )?;
            sink.write_all(&block[..len])?;
            pos += len;
        }
        require(pos == wire.len(), "encoder stopped before final frame")?;
        Ok(pos)
    })();
    let encoded = tx.wait()?;
    let decoded = rx.wait()?;
    let size = transmission?;
    require(
        encoded.success() && decoded.success(),
        format!("pipe process status: encode={encoded}, decode={decoded}"),
    )?;
    require(
        fs::read(output.path())? == payload,
        "decoded bytes differ from input",
    )?;
    Ok(
        json!({"encodeExit":encoded.code(),"decodeExit":decoded.code(),"wireBytesCompared":size,"payloadBytesCompared":payload.len()}),
    )
}

fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let generated = args.generated.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    let lock = generated.join("workspace.lock");
    let workspace_sha = sha(&lock)?;
    let profile_lock = "889017343f0fdb092dee203264aa4f6d6d194bad8ef396e336f2dd2cf6ec9b76";
    require(
        [LOCK, profile_lock].contains(&workspace_sha.as_str()),
        "wrong generated workspace lock",
    )?;
    check_members(&generated, &load(&lock)?, false)?;
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let a = build(&work.join("original"), &original, "crates/marmish-stream")?;
    let b = build(&work.join("generated"), &generated, "marmish-stream")?;
    let payload = corpus();
    let (wire, frames) = expected(&payload);
    let input = work.join("input");
    fs::write(&input, &payload)?;
    let forward = exchange(&a, &b, &input, &payload, &wire)?;
    let reverse = exchange(&b, &a, &input, &payload, &wire)?;
    let mut invalid_offset = wire[..14].to_vec();
    invalid_offset[12] = 1;
    for decoder in [&a, &b] {
        for (bytes, reason) in [
            (&wire[..wire.len() - 1], "UnexpectedEof"),
            (invalid_offset.as_slice(), "expected contiguous Data frames"),
        ] {
            let r = capture(Command::new(decoder).arg("decode"), 60, Some(bytes))?;
            require(
                !r.status.success() && String::from_utf8_lossy(&r.stderr).contains(reason),
                "decoder accepted malformed stream or failed for the wrong reason",
            )?;
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"original":ORIGINAL,"generatedWorkspaceLockSha256":workspace_sha,"driverSha256":bytes_sha(include_bytes!("pipe.rs")),"framesPerDirection":frames,"payloadSha256":bytes_sha(&payload),"wireSha256":bytes_sha(&wire),"originalToGenerated":forward,"generatedToOriginal":reverse,"truncationAndOffsetControlsPassed":4})
        )?
    );
    Ok(())
}
