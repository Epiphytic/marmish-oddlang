#[path = "../../converged-public-api/compose.rs"]
pub mod api_composition;
#[path = "../../mdk-json-input/compose.rs"]
mod mdk_seam;
#[path = "../../runtime-status-data/compose.rs"]
mod runtime_status_composition;
pub mod assembly;
#[path = "../../unified-buffered/buffer.rs"]
pub mod buffered;
#[path = "../../canonical-range-data/compose.rs"]
pub mod canonical_composition;
#[path = "../compose.rs"]
pub mod composition;
#[path = "../domains.rs"]
pub mod domains;
#[path = "../imports.rs"]
pub mod imports;
#[path = "../../converged-release-manifest/native.rs"]
pub mod manifest_native;
#[path = "../../converged-release-manifest/product.rs"]
pub mod manifest_product;
#[path = "../../mdk-public-data/member.rs"]
pub mod mdk;
#[path = "../product.rs"]
pub mod product;
#[path = "../../unified-mdk/compose.rs"]
pub mod six_composition;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn whole() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}
pub fn portable() -> PathBuf {
    whole().ancestors().nth(2).unwrap().join("oddlang/portable")
}
pub fn require(ok: bool, msg: impl Into<String>) -> Result {
    if ok { Ok(()) } else { Err(msg.into().into()) }
}
pub fn sha(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}
pub fn load(path: &Path) -> Result<Value> {
    Ok(serde_json::from_reader(fs::File::open(path)?)?)
}
pub fn text(v: &Value) -> Result<&str> {
    Ok(v.as_str().ok_or("expected string")?)
}
pub fn list(v: &Value) -> Result<&Vec<Value>> {
    Ok(v.as_array().ok_or("expected array")?)
}
pub fn json_bytes(v: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for ch in serde_json::to_string_pretty(v)?.chars() {
        if ch < '\u{7f}' {
            bytes.push(ch as u8);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                bytes.extend(format!("\\u{unit:04x}").bytes());
            }
        }
    }
    bytes.push(b'\n');
    Ok(bytes)
}
pub fn pin(base: &Path, relative: &str) -> Result<Value> {
    Ok(serde_json::json!({"path":relative,"sha256":sha(&base.join(relative))?}))
}
/// Capture both streams without pipe backpressure and bound each native tool invocation.
pub fn capture(
    cmd: &mut Command,
    seconds: u64,
    input: Option<&[u8]>,
) -> Result<std::process::Output> {
    use std::io::{Read, Seek, Write};
    use std::os::unix::process::CommandExt;
    use std::process::{Output, Stdio};
    use std::time::{Duration, Instant};
    let mut out = tempfile::tempfile()?;
    let mut err = tempfile::tempfile()?;
    if let Some(bytes) = input {
        let mut file = tempfile::tempfile()?;
        file.write_all(bytes)?;
        file.rewind()?;
        cmd.stdin(file);
    } else {
        cmd.stdin(Stdio::null());
    }
    // The downstream callback owns a dedicated session and cancels its complete group on failure.
    let supervised = std::env::var_os("MARMISH_WORKSPACE_CALLBACK_SESSION").is_some()
        && unsafe { libc::getsid(0) } == std::process::id() as libc::pid_t;
    if !supervised {
        cmd.process_group(0);
    }
    let mut child = cmd
        .stdout(out.try_clone()?)
        .stderr(err.try_clone()?)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            if supervised {
                child.kill()?;
            } else {
                // This child was made leader of its own group immediately before exec.
                unsafe {
                    libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
                }
            }
            child.wait()?;
            return Err(format!("{cmd:?} timed out after {seconds}s").into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    out.rewind()?;
    err.rewind()?;
    out.read_to_end(&mut stdout)?;
    err.read_to_end(&mut stderr)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
pub fn run(cmd: &mut Command) -> Result<Vec<u8>> {
    let output = capture(cmd, 600, None)?;
    require(
        output.status.success(),
        format!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(output.stdout)
}
pub fn stdout(cmd: &mut Command) -> Result<String> {
    Ok(String::from_utf8(run(cmd)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_preserves_insertion_order_and_ascii_escaping() -> Result {
        let json = serde_json::json!({"z": "🌊", "a": "é"});
        let expected = "{\n  \"z\": \"\\ud83c\\udf0a\",\n  \"a\": \"\\u00e9\"\n}\n";
        assert_eq!(json_bytes(&json)?, expected.as_bytes());
        Ok(())
    }
}

#[path = "../../converged-release-manifest/inherited.rs"]
mod manifest_inherited;

pub mod artifacts;
#[path = "../../canonical-range-data/qualification.rs"]
pub mod range;
pub mod private_buffer;
