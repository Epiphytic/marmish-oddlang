// Generated from the locked Oddlang framing contract.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits;

impl Limits {
    pub const RAW_BYTES: usize = 16384;
    pub const JSON_BYTES: usize = 32768;
    pub const NDJSON_BYTES: usize = 65536;
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("probe frame exceeds its size limit")]
    SizeLimit,
    #[error("invalid probe JSON")]
    InvalidJson,
    #[error("unsupported probe version")]
    InvalidVersion,
    #[error("invalid canonical base64 payload")]
    InvalidEncoding,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFrame {
    version: u32,
    payload: String,
}

pub struct ProbeEnvelope;

impl ProbeEnvelope {
    pub fn encode(input: &[u8]) -> Result<Vec<u8>, FrameError> {
        if input.len() > Limits::RAW_BYTES { return Err(FrameError::SizeLimit); }
        let record = WireFrame { version: 1, payload: STANDARD.encode(input) };
        let frame = serde_json::to_vec(&record).map_err(|_| FrameError::InvalidJson)?;
        if frame.len() > Limits::JSON_BYTES { return Err(FrameError::SizeLimit); }
        Ok(frame)
    }

    pub fn decode(input: &[u8]) -> Result<Vec<u8>, FrameError> {
        if input.len() > Limits::JSON_BYTES { return Err(FrameError::SizeLimit); }
        let record: WireFrame = serde_json::from_slice(input).map_err(|_| FrameError::InvalidJson)?;
        if record.version != 1 { return Err(FrameError::InvalidVersion); }
        if record.payload.len() > Limits::RAW_BYTES.div_ceil(3) * 4 { return Err(FrameError::SizeLimit); }
        let payload = STANDARD.decode(&record.payload).map_err(|_| FrameError::InvalidEncoding)?;
        if payload.len() > Limits::RAW_BYTES { return Err(FrameError::SizeLimit); }
        if STANDARD.encode(&payload) != record.payload { return Err(FrameError::InvalidEncoding); }
        Ok(payload)
    }
}
