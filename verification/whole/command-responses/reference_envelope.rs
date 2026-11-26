use crate::{codec, Result};

pub(crate) fn cycle(_: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    codec::encode_response(&codec::decode_response(bytes)?)
}
