use crate::{codec, Error, Result};
use serde::{de::DeserializeOwned, Serialize};

fn round_trip<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<Vec<u8>> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| Error::Invalid("response"))?;
    serde_json::to_vec(&value).map_err(|_| Error::Invalid("response"))
}

pub(crate) fn cycle(kind: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    match kind {
        "chunk" => round_trip::<codec::Chunk>(bytes),
        "status" => round_trip::<codec::Status>(bytes),
        "error" => round_trip::<codec::WireError>(bytes),
        "outcome" => round_trip::<codec::Outcome>(bytes),
        "gap" => round_trip::<codec::Gap>(bytes),
        "disposition" => round_trip::<codec::Disposition>(bytes),
        _ => Err(Error::Invalid("unknown response shape")),
    }
}
