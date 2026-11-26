use marmish_stream::{
    codec::Codec,
    copy_protocol::{AttemptMode, Direction, Frame, Launch, Manifest, Mtime},
};

fn launch(version: u32, valid_path: bool) -> Launch {
    Launch {
        version,
        mode: AttemptMode::Create,
        transfer: [0x12; 32],
        client: [0x34; 32],
        server: [0x56; 32],
        direction: Direction::Put,
        source: if valid_path { vec![0xab, 0xcd] } else { vec![] },
        destination: vec![0xe1],
        overwrite: true,
        preserve_mode: false,
        preserve_mtime: false,
    }
}
fn manifest(version: u32, valid_path: bool, metadata: bool, invalid: bool) -> Manifest {
    let value = launch(version, valid_path);
    Manifest {
        version: value.version,
        transfer: value.transfer,
        client: value.client,
        server: value.server,
        direction: value.direction,
        source: value.source,
        destination: value.destination,
        overwrite: true,
        preserve_mode: metadata,
        preserve_mtime: metadata,
        size: 0x0213,
        sha256: [0x78; 32],
        mode: metadata.then_some(if invalid { 0 } else { 0o640 }),
        mtime: metadata.then_some(Mtime {
            seconds: -3,
            nanos: 9,
        }),
    }
}

fn main() {
    for value in [
        Frame::Start(launch(1, true), Codec::None),
        Frame::Start(launch(1, true), Codec::Zlib),
        Frame::Start(launch(0, false), Codec::Zlib),
        Frame::Start(launch(1, false), Codec::None),
        Frame::Manifest(manifest(1, true, false, false)),
        Frame::Manifest(manifest(1, true, true, false)),
        Frame::Manifest(manifest(0, false, true, true)),
        Frame::Manifest(manifest(1, false, true, true)),
        Frame::Manifest(manifest(1, true, true, true)),
    ] {
        match value.encode() {
            Ok(data) => println!(
                "{}",
                data.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ),
            Err(error) => println!("{error:?}"),
        }
    }
}
