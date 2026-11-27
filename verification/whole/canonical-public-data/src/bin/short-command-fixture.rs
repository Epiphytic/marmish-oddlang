//! Executable for the five closed short cases in the basic command oracle.
use public_data_qualification::{Result, assembly::bytes_sha, require, sha};
use serde_json::json;
use std::{
    env,
    fs::File,
    io::{Read, Write},
    os::unix::ffi::OsStrExt,
    process::ExitCode,
};

fn fixture() -> Result<u8> {
    let argv = env::args_os().collect::<Vec<_>>();
    let [_, name, nonce] = argv.as_slice() else {
        return Ok(125);
    };
    let nonce = nonce.as_encoded_bytes();
    if nonce.len() != 64 || !nonce.iter().all(|c| matches!(c,b'0'..=b'9'|b'a'..=b'f')) {
        return Ok(125);
    }
    let nonce = std::str::from_utf8(nonce)?;
    let (input, out, err, status): (&[u8], &[u8], &[u8], u8) = match name.to_str() {
        Some("success") => (b"hello", b"done\n", b"", 0),
        Some("separate-streams") => (b"\0\xfe\n", b"stdout\0\xff", b"stderr\0\xfe", 0),
        Some("empty") => (b"", b"", b"", 0),
        Some("trailing-newlines") => (b"next\n", b"one\r\n\n", b"two", 0),
        Some("nonzero") => (b"finish", b"partial\n", b"controlled error\n", 7),
        _ => return Ok(125),
    };
    let mut data = Vec::new();
    std::io::stdin()
        .take(input.len() as u64 + 1)
        .read_to_end(&mut data)?;
    if data != input {
        return Ok(125);
    }
    let exe = env::current_exe()?.canonicalize()?;
    let mut encoded = Vec::new();
    for arg in std::iter::once(exe.as_os_str()).chain(argv[1..].iter().map(|s| s.as_os_str())) {
        encoded.extend_from_slice(&u32::try_from(arg.as_bytes().len())?.to_be_bytes());
        encoded.extend_from_slice(arg.as_bytes());
    }
    let receipt = json!({"schema":1,"case":name.to_str(),"nonce":nonce,"executable_sha256":sha(&exe)?,"argv_sha256":bytes_sha(&encoded),
        "stdin_bytes":data.len(),"stdin_sha256":bytes_sha(&data),"stdout_bytes":out.len(),"stdout_sha256":bytes_sha(out),
        "stderr_bytes":err.len(),"stderr_sha256":bytes_sha(err),"exit":status});
    let receipts = exe
        .parent()
        .ok_or("missing fixture directory")?
        .join("receipts");
    require(receipts.is_dir(), "missing receipts directory")?;
    std::io::stdout().lock().write_all(out)?;
    std::io::stderr().lock().write_all(err)?;
    serde_json::to_writer(
        File::create_new(receipts.join(format!("{nonce}.receipt")))?,
        &receipt,
    )?;
    Ok(status)
}
fn main() -> ExitCode {
    match fixture() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
