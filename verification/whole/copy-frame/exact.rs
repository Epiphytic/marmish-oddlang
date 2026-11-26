// Additional tests exercise error identity and precedence omitted by the pinned suite.
use marmish_stream::{
    codec::Codec,
    copy_protocol::{
        AttemptMode, Direction, Frame, FrameDecoder, Launch, Manifest, MAX_CONTROL, VERSION,
    },
    Error, MAX_BLOCK, MAX_OFFSET,
};

fn wire(tag: u8, body: &[u8]) -> Vec<u8> {
    [
        vec![tag],
        (body.len() as u32).to_be_bytes().to_vec(),
        body.to_vec(),
    ]
    .concat()
}

fn launch() -> Launch {
    Launch {
        version: VERSION,
        mode: AttemptMode::Create,
        transfer: [1; 32],
        client: [2; 32],
        server: [3; 32],
        direction: Direction::Put,
        source: vec![0xff],
        destination: vec![0x80],
        overwrite: false,
        preserve_mode: false,
        preserve_mtime: false,
    }
}

fn manifest() -> Manifest {
    Manifest {
        version: VERSION,
        transfer: [1; 32],
        client: [2; 32],
        server: [3; 32],
        direction: Direction::Put,
        source: vec![0xff],
        destination: vec![0x80],
        overwrite: false,
        preserve_mode: false,
        preserve_mtime: false,
        size: 3,
        sha256: [0x11; 32],
        mode: None,
        mtime: None,
    }
}

fn take_error(bytes: &[u8], error: Error) {
    assert_eq!(Frame::decode(bytes).err(), Some(error));
}

#[test]
fn frame_headers_precede_tag_and_slot_checks() {
    for length in 0..5 {
        take_error(&[255u8; 5][..length], Error::Invalid("copy_frame"));
    }
    take_error(&[255, 255, 255, 255, 255], Error::Limit);
    take_error(&[255, 0, 0, 0, 1], Error::Invalid("copy_frame_length"));
    take_error(&[255, 0, 0, 0, 0], Error::Invalid("copy_frame_tag"));
    take_error(&[255, 0, 0, 0, 0, 0], Error::Invalid("copy_frame_length"));
    for tag in [0, 12] {
        take_error(&wire(tag, &[]), Error::Invalid("copy_frame_tag"));
    }
    take_error(
        &wire(255, &vec![0; MAX_CONTROL]),
        Error::Invalid("copy_frame_tag"),
    );
    take_error(&wire(255, &vec![0; MAX_CONTROL + 1]), Error::Limit);
    take_error(&wire(11, &[255]), Error::Invalid("copy_enum"));
    take_error(&wire(11, &[1]), Error::Invalid("copy_truncated"));
    take_error(&wire(11, &[1, 1]), Error::Invalid("copy_error_owner"));
    take_error(&wire(11, &[1, 0, 1]), Error::Invalid("copy_trailing"));
}

#[test]
fn inline_validation_precedes_codec_and_trailing() {
    let mut start = Frame::Start(launch(), Codec::None).encode().unwrap()[5..].to_vec();
    let codec = start.len() - 1;
    start[codec] = 2;
    take_error(&wire(1, &start), Error::Invalid("copy_codec"));
    take_error(&wire(1, &start[..codec]), Error::Invalid("copy_truncated"));
    start[4 + 1 + 96 + 1 + 4] = 0;
    take_error(&wire(1, &start), Error::Invalid("copy_path"));
    take_error(&wire(1, &start[..codec]), Error::Invalid("copy_path"));
    let mut m = manifest().encode().unwrap();
    m[3] = 2;
    m.push(0);
    take_error(&wire(2, &m), Error::Invalid("copy_version"));
    assert_eq!(
        Manifest::decode(&m).err(),
        Some(Error::Invalid("copy_trailing"))
    );
}

#[test]
fn data_and_progress_guards_have_directional_precedence() {
    assert_eq!(
        Frame::Data {
            offset: MAX_OFFSET + 1,
            bytes: vec![]
        }
        .encode()
        .err(),
        Some(Error::Limit)
    );
    take_error(&wire(5, &(MAX_OFFSET + 1).to_be_bytes()), Error::Offset);
    take_error(&wire(5, &MAX_OFFSET.to_be_bytes()), Error::Limit);
    take_error(
        &wire(5, &[&MAX_OFFSET.to_be_bytes()[..], &[1]].concat()),
        Error::Offset,
    );
    let too_big = vec![6; MAX_BLOCK + 1];
    take_error(
        &wire(5, &[&MAX_OFFSET.to_be_bytes()[..], &too_big].concat()),
        Error::Limit,
    );
    let good = Frame::Data {
        offset: MAX_OFFSET - 1,
        bytes: vec![7],
    };
    let encoded = good.encode().unwrap();
    assert_eq!(
        encoded,
        wire(5, &[&(MAX_OFFSET - 1).to_be_bytes()[..], &[7]].concat())
    );
    assert!(Frame::decode(&encoded).unwrap() == good);
    take_error(
        &wire(
            10,
            &[&[1], &2u64.to_be_bytes()[..], &1u64.to_be_bytes()[..], &[0]].concat(),
        ),
        Error::Offset,
    );
    take_error(
        &wire(10, &[&[1], &(MAX_OFFSET + 1).to_be_bytes()[..]].concat()),
        Error::Offset,
    );
}

#[test]
fn commit_constant_and_poisoned_decoder_outcomes_are_exact() {
    let mut commit = Frame::Commit {
        transfer: [0x11; 32],
        size: 1,
        sha256: [0x22; 32],
    }
    .encode()
    .unwrap()[5..]
        .to_vec();
    let last = commit.len() - 1;
    take_error(&wire(8, &commit[..last]), Error::Invalid("copy_truncated"));
    commit[last] = 2;
    take_error(&wire(8, &commit), Error::Invalid("copy_disposition"));
    let mut decoder = FrameDecoder::default();
    assert!(decoder.feed(&[255, 0, 0, 0]).unwrap() == (4, None));
    assert_eq!(decoder.finish(), Err(Error::Invalid("copy_truncated")));
    assert_eq!(
        decoder.feed(&[0]).err(),
        Some(Error::Invalid("copy_frame_tag"))
    );
    assert_eq!(
        decoder.feed(&[]).err(),
        Some(Error::Invalid("copy_decoder_failed"))
    );
    assert_eq!(decoder.finish(), Err(Error::Invalid("copy_truncated")));
    let mut next = FrameDecoder::default();
    assert!(next.feed(&[1]).unwrap() == (1, None));
    assert_eq!(
        next.feed(&((MAX_CONTROL as u32) + 1).to_be_bytes()).err(),
        Some(Error::Limit)
    );
    assert_eq!(next.finish(), Err(Error::Invalid("copy_truncated")));
}

#[test]
fn public_copy_protocol_constants_and_signatures() {
    use marmish_stream::copy_protocol::*;
    let constants: (
        u32,
        usize,
        usize,
        u64,
        u64,
        i64,
        u64,
        u32,
        usize,
        &'static str,
    ) = (
        VERSION,
        MAX_PATH,
        MAX_CONTROL,
        DURABLE_WINDOW,
        CHECKPOINT_INTERVAL,
        MIN_RETENTION_SECS,
        MAX_RETENTION_SECS,
        CONTEXT_VERSION,
        MAX_CONTEXT,
        HELPER_ENTRYPOINT,
    );
    assert_eq!(
        constants,
        (
            1,
            4096,
            16384,
            2097152,
            1048576,
            604860,
            31536000,
            1,
            4096,
            "__copy-helper-v1"
        )
    );
    let _: fn(&Launch, &Manifest) -> bool = Launch::matches_manifest;
    let _: fn(&Launch, Codec) -> marmish_stream::Result<[u8; 32]> = Launch::digest;
    let _: fn(&Launch) -> String = Launch::namespace;
    let _: fn(&HelperContext, &Launch, Codec) -> marmish_stream::Result<()> =
        HelperContext::validate_start;
    let _: fn(&HelperContext, &str, &str, [u8; 20]) -> marmish_stream::Result<()> =
        HelperContext::validate_build;
    let _: fn(&Checkpoint) -> marmish_stream::Result<()> = Checkpoint::validate;
    let _: fn(&Frame) -> marmish_stream::Result<Vec<u8>> = Frame::encode;
    let _: fn(&[u8]) -> marmish_stream::Result<Frame> = Frame::decode;
    let _: fn(&mut FrameDecoder, &[u8]) -> marmish_stream::Result<(usize, Option<Frame>)> =
        FrameDecoder::feed;
    let _: fn(&FrameDecoder) -> marmish_stream::Result<()> = FrameDecoder::finish;
}

#[test]
fn all_case_tags_and_canonical_payloads() {
    use marmish_stream::copy_protocol::{CopyError, Phase, Stage};
    let mut raw_launch = Vec::new();
    raw_launch.extend_from_slice(&1u32.to_be_bytes());
    raw_launch.push(1);
    raw_launch.extend_from_slice(&[[1; 32], [2; 32], [3; 32]].concat());
    raw_launch.push(1);
    raw_launch.extend_from_slice(&[0, 0, 0, 1, 255, 0, 0, 0, 1, 128, 0, 0, 0, 1]);
    let start = Frame::Start(launch(), Codec::Zlib);
    let manifest = manifest();
    let manifest_payload = manifest.encode().unwrap();
    let count = 7u64.to_be_bytes();
    let hash = [0x44u8; 32];
    let transfer = [0x77u8; 32];
    let mut offset_hash = count.to_vec();
    offset_hash.extend_from_slice(&hash);
    let expected = [
        (start, 1, raw_launch),
        (Frame::Manifest(manifest), 2, manifest_payload),
        (
            Frame::Resume {
                phase: Phase::Receiving,
                offset: 7,
                sha256: hash,
            },
            3,
            [&[2], offset_hash.as_slice()].concat(),
        ),
        (
            Frame::Prefix {
                offset: 7,
                sha256: hash,
            },
            4,
            offset_hash.clone(),
        ),
        (
            Frame::Data {
                offset: 7,
                bytes: vec![0, 255],
            },
            5,
            [&count[..], &[0, 255]].concat(),
        ),
        (
            Frame::Checkpoint {
                offset: 7,
                sha256: hash,
            },
            6,
            offset_hash.clone(),
        ),
        (
            Frame::Done {
                size: 7,
                sha256: hash,
            },
            7,
            offset_hash.clone(),
        ),
        (
            Frame::Commit {
                transfer,
                size: 7,
                sha256: hash,
            },
            8,
            [&transfer[..], &offset_hash, &[1]].concat(),
        ),
        (
            Frame::Finish {
                transfer,
                sha256: hash,
            },
            9,
            [transfer, hash].concat(),
        ),
        (
            Frame::Progress {
                stage: Stage::Hashing,
                examined: 7,
                total: 9,
            },
            10,
            [&[1], &count[..], &9u64.to_be_bytes()[..]].concat(),
        ),
        (Frame::Error(CopyError::Busy), 11, vec![6, 0]),
    ];
    for (frame, tag, payload) in expected {
        let bytes = wire(tag, &payload);
        assert_eq!(frame.encode().unwrap(), bytes);
        assert!(Frame::decode(&bytes).unwrap() == frame);
    }
}

#[test]
fn launch_fragment_reports_early_read_errors() {
    let valid = Frame::Start(launch(), Codec::None).encode().unwrap()[5..].to_vec();
    let source_len = 4 + 1 + 3 * 32 + 1;
    let destination_len = source_len + 5;
    let overwrite = destination_len + 5;
    let mut wrong_version = valid.clone();
    wrong_version[3] = 2;
    take_error(
        &wire(1, &wrong_version[..destination_len + 4]),
        Error::Invalid("copy_truncated"),
    );
    wrong_version[overwrite] = 2;
    take_error(&wire(1, &wrong_version), Error::Invalid("copy_boolean"));
    for position in [4, 4 + 1 + 3 * 32] {
        let mut body = valid.clone();
        body[position] = 0;
        take_error(&wire(1, &body), Error::Invalid("copy_enum"));
    }
    let mut too_long = valid.clone();
    too_long[source_len..source_len + 4].copy_from_slice(&4097u32.to_be_bytes());
    take_error(&wire(1, &too_long), Error::Limit);
    let mut invalid_path = valid.clone();
    invalid_path[source_len + 4] = 0;
    invalid_path[destination_len..destination_len + 4].copy_from_slice(&4097u32.to_be_bytes());
    take_error(&wire(1, &invalid_path), Error::Limit);
    let mut empty = launch();
    empty.source.clear();
    assert_eq!(
        Frame::Start(empty, Codec::None).encode().err(),
        Some(Error::Invalid("copy_path"))
    );
    let mut incorrect = launch();
    incorrect.version = 2;
    assert_eq!(
        Frame::Start(incorrect, Codec::None).encode().err(),
        Some(Error::Invalid("copy_version"))
    );
}

#[test]
fn remaining_case_errors_preserve_exact_order() {
    use marmish_stream::copy_protocol::{Phase, Stage};
    let offset = MAX_OFFSET + 1;
    let hash = [1; 32];
    for value in [
        Frame::Resume {
            phase: Phase::Receiving,
            offset,
            sha256: hash,
        },
        Frame::Prefix {
            offset,
            sha256: hash,
        },
        Frame::Checkpoint {
            offset,
            sha256: hash,
        },
        Frame::Done {
            size: offset,
            sha256: hash,
        },
        Frame::Commit {
            transfer: hash,
            size: offset,
            sha256: hash,
        },
    ] {
        assert_eq!(value.encode().err(), Some(Error::Offset));
    }
    let number = offset.to_be_bytes();
    take_error(&wire(3, &[&[2], &number[..]].concat()), Error::Offset);
    for tag in [4, 6, 7] {
        take_error(&wire(tag, &number), Error::Offset);
    }
    take_error(&wire(8, &[&hash[..], &number[..]].concat()), Error::Offset);
    for phase in [0, 6] {
        take_error(&wire(3, &[phase]), Error::Invalid("copy_enum"));
    }
    for stage in [0, 5] {
        take_error(&wire(10, &[stage]), Error::Invalid("copy_enum"));
    }
    take_error(&wire(5, &[0; 7]), Error::Invalid("copy_truncated"));
    for value in [
        Frame::Finish {
            transfer: hash,
            sha256: hash,
        },
        Frame::Resume {
            phase: Phase::Receiving,
            offset: 1,
            sha256: hash,
        },
        Frame::Commit {
            transfer: hash,
            size: 1,
            sha256: hash,
        },
        Frame::Progress {
            stage: Stage::Hashing,
            examined: 1,
            total: 1,
        },
    ] {
        let encoded = value.encode().unwrap();
        take_error(
            &wire(encoded[0], &[&encoded[5..], &[0]].concat()),
            Error::Invalid("copy_trailing"),
        );
    }
    let mut m = manifest();
    m.preserve_mode = true;
    assert_eq!(
        Frame::Manifest(m).encode().err(),
        Some(Error::Invalid("copy_metadata"))
    );
    let mut m = manifest().encode().unwrap();
    m[3] = 2;
    let offset_slot = 4 + 96 + 1 + 2 * 5 + 3;
    m[offset_slot..offset_slot + 8].copy_from_slice(&offset.to_be_bytes());
    take_error(&wire(2, &m), Error::Offset);
    m[offset_slot..offset_slot + 8].fill(0);
    m[offset_slot + 8 + 32] = 2;
    take_error(&wire(2, &m), Error::Invalid("copy_option"));
}

#[test]
fn data_decode_rejects_offset_before_empty_body() {
    take_error(&wire(5, &(MAX_OFFSET + 1).to_be_bytes()), Error::Offset);
}
