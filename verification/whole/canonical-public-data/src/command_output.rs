//! Exact process status, stdout and stderr observations.
use crate::{Result, assembly::bytes_sha, require};
use regex::bytes::Regex;
use serde_json::{Value, json};
pub struct Output<'a> {
    pub status: i32,
    pub stdout: &'a [u8],
    pub stderr: &'a [u8],
}
impl Output<'_> {
    pub fn compare(&self, other: &Self) -> Result {
        require(
            self.status == other.status,
            "command status differs from the reference",
        )?;
        require(
            self.stdout == other.stdout,
            "command stdout differs from the reference",
        )?;
        require(
            self.stderr == other.stderr,
            "command stderr differs from the reference",
        )
    }
    pub fn handle(&self, server: &str) -> Result<&[u8]> {
        let grammar = Regex::new(concat!(
            r#"\Amarmish handle: (\{"format":1,"profile":"client","remote":"([0-9a-f]{64})","#,
            r#""session":"[0-9a-f]{32}","invocation":"[0-9a-f]{32}","submit_sha256":"[0-9a-f]{64}"\})\n\z"#
        ))?;
        require(
            self.status == 0 && self.stdout.is_empty(),
            "invalid detached response",
        )?;
        let captures = grammar
            .captures(self.stderr)
            .ok_or("invalid detached response")?;
        require(
            &captures[2] == server.as_bytes(),
            "detached response names a different server",
        )?;
        Ok(captures.get(1).unwrap().as_bytes())
    }
    pub fn receipt(&self) -> Value {
        let stream = |s: &[u8]| json!({"bytes":s.len(),"sha256":bytes_sha(s)});
        json!({"status":self.status,"stdout":stream(self.stdout),"stderr":stream(self.stderr)})
    }
    pub fn goodput(&self, expected: &Self, ns: u64) -> Result<f64> {
        self.compare(expected)?;
        require(
            self.stdout.len() + self.stderr.len() == 262144,
            "unexpected finite benchmark payload size",
        )?;
        require(
            (1..=330_000_000_000).contains(&ns),
            "invalid finite benchmark duration",
        )?;
        Ok(262_144.0 * 1_000_000_000.0 / ns as f64)
    }
}
