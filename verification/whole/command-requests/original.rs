use crate::{Result, codec};

pub(crate) fn cycle(_op: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    codec::encode_request(&codec::decode_request(bytes)?)
}
