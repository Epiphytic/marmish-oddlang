use crate::{Result, codec::*};

pub(crate) fn cycle(op: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    match op {
        "submit" => encode_submit(&decode_submit(bytes)?),
        "status" => encode_status(&decode_status(bytes)?),
        "poll" => encode_poll(&decode_poll(bytes)?),
        "cancel" => encode_cancel(&decode_cancel(bytes)?),
        "finish" => encode_finish(&decode_finish(bytes)?),
        _ => panic!("unknown corpus case"),
    }
}
