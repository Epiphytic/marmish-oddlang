use marmish_core::{ConfigError, FrameError, Limits, ProbeEnvelope, RelaySet, Retention};

const _: u64 = Retention::MAX_SECONDS;
const _: usize = Limits::RAW_BYTES;
const _: usize = Limits::JSON_BYTES;
const _: usize = Limits::NDJSON_BYTES;

const _: fn(Option<u64>) -> Result<Retention, ConfigError> = Retention::from_seconds;
const _: fn(&Retention) -> Option<u64> = Retention::seconds;
const _: fn(&[String], bool) -> Result<RelaySet, ConfigError> = RelaySet::parse;
const _: fn(&RelaySet) -> &[String] = RelaySet::as_slice;
const _: fn(&[u8]) -> Result<Vec<u8>, FrameError> = ProbeEnvelope::encode;
const _: fn(&[u8]) -> Result<Vec<u8>, FrameError> = ProbeEnvelope::decode;
