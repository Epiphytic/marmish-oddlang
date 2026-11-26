use marmish_stream::copy_protocol::{Frame, Phase, Stage};

const EDGE: u64 = (1_u64 << 63) - 1;
const BLOCK: usize = 8192;

fn encoded(frame: &Frame) -> String {
    match frame.encode() {
        Ok(bytes) => bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        Err(error) => format!("{error:?}"),
    }
}

fn main() {
    for (offset, count) in [
        (EDGE + 1, 0),
        (EDGE + 1, BLOCK + 1),
        (0, 0),
        (0, BLOCK + 1),
        (EDGE, 1),
        (EDGE - 1, 1),
        (u64::MAX, 1),
        (EDGE - 2, 2),
        (EDGE, BLOCK + 1),
        (0, 3),
        (0, BLOCK),
    ] {
        let bytes = vec![0x76; count];
        let frame = Frame::Data {
            offset,
            bytes: bytes.clone(),
        };
        let mut wire = vec![5];
        wire.extend(((count + 8) as u32).to_be_bytes());
        wire.extend(offset.to_be_bytes());
        wire.extend(bytes);
        let decoded = match Frame::decode(&wire) {
            Ok(decoded) => {
                assert!(frame == decoded);
                "OK".into()
            }
            Err(error) => format!("{error:?}"),
        };
        println!("{} {decoded}", encoded(&frame));
    }
    let sha256 = [0x21; 32];
    let transfer = [0x13; 32];
    for frame in [
        Frame::Resume {
            phase: Phase::Receiving,
            offset: EDGE + 1,
            sha256,
        },
        Frame::Prefix {
            offset: EDGE + 1,
            sha256,
        },
        Frame::Checkpoint {
            offset: EDGE + 1,
            sha256,
        },
        Frame::Done {
            size: EDGE + 1,
            sha256,
        },
        Frame::Commit {
            transfer,
            size: EDGE + 1,
            sha256,
        },
        Frame::Progress {
            stage: Stage::PrefixHash,
            examined: 0,
            total: EDGE + 1,
        },
        Frame::Progress {
            stage: Stage::PrefixHash,
            examined: 2,
            total: 1,
        },
    ] {
        println!("{}", encoded(&frame));
    }
}
