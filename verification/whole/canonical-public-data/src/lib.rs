#[path = "../../mdk-public-data/member.rs"]
pub mod mdk;
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
pub fn run(cmd: &mut Command) -> Result<Vec<u8>> {
    let output = cmd.output()?;
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
