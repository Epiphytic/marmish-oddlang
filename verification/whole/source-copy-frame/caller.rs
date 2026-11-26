use marmish_stream::{
    codec::Codec,
    copy_protocol::{AttemptMode, CopyError, Direction, Frame, Launch, Manifest, Phase, Stage},
};

fn main() {
    let transfer = [0x22; 32];
    let sha256 = [0x45; 32];
    let launch = Launch {
        version: 1,
        mode: AttemptMode::Create,
        transfer,
        client: [0x38; 32],
        server: [0x74; 32],
        direction: Direction::Put,
        source: vec![0xf1, 0x81],
        destination: vec![0xde],
        overwrite: true,
        preserve_mode: false,
        preserve_mtime: false,
    };
    let manifest = Manifest {
        version: launch.version,
        transfer,
        client: launch.client,
        server: launch.server,
        direction: launch.direction,
        source: launch.source.clone(),
        destination: launch.destination.clone(),
        overwrite: launch.overwrite,
        preserve_mode: false,
        preserve_mtime: false,
        size: 0x0654,
        sha256,
        mode: None,
        mtime: None,
    };
    for frame in [
        Frame::Start(launch, Codec::Zlib),
        Frame::Manifest(manifest),
        Frame::Resume {
            phase: Phase::Receiving,
            offset: 0x1234,
            sha256,
        },
        Frame::Prefix {
            offset: 0x1334,
            sha256,
        },
        Frame::Data {
            offset: 0x16,
            bytes: vec![0x81, 0x26, 0x59],
        },
        Frame::Checkpoint {
            offset: 0x1534,
            sha256,
        },
        Frame::Done {
            size: 0x1634,
            sha256,
        },
        Frame::Commit {
            transfer,
            size: 0x1734,
            sha256,
        },
        Frame::Finish { transfer, sha256 },
        Frame::Progress {
            stage: Stage::PrefixHash,
            examined: 0x07,
            total: 0x0b,
        },
        Frame::Error(CopyError::HashMismatch),
    ] {
        for byte in frame.encode().unwrap() {
            print!("{byte:02x}");
        }
        println!();
    }
}
