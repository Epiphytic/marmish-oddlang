use crate::{codec::Chunk, Error, Result};

pub(crate) fn cycle(_: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let chunk: Chunk = serde_json::from_slice(bytes).map_err(|_| Error::Invalid("chunk"))?;
    serde_json::to_vec(&chunk).map_err(|_| Error::Invalid("chunk"))
}
