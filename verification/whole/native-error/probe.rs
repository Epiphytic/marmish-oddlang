use marmish_native::error::{Error, Result};
use marmish_stream::{copy_protocol::CopyError, wire::WireErrorCode};
use std::error::Error as _;

fn category_name(error: &Error) -> &'static str {
    match error {
        Error::DaemonStartup(_) => "DaemonStartup",
        Error::Io(_) => "Io",
        Error::Invalid(_) => "Invalid",
        Error::Identity(_) => "Identity",
        Error::Storage(_) => "Storage",
        Error::Transport(_) => "Transport",
        Error::Mdk(_) => "Mdk",
        Error::Copy(_) => "Copy",
        Error::Stream(_) => "Stream",
        Error::Timeout => "Timeout",
    }
}

fn copy_name(code: &CopyError) -> &'static str {
    match code {
        CopyError::Unsupported => "Unsupported",
        CopyError::Protocol => "Protocol",
        CopyError::PermissionDenied => "PermissionDenied",
        CopyError::NotFound => "NotFound",
        CopyError::UnsafePath => "UnsafePath",
        CopyError::Busy => "Busy",
        CopyError::Conflict => "Conflict",
        CopyError::SourceChanged => "SourceChanged",
        CopyError::HashMismatch => "HashMismatch",
        CopyError::DiskFull => "DiskFull",
        CopyError::Quota => "Quota",
        CopyError::CommitUncertain => "CommitUncertain",
        CopyError::LocalIo => "LocalIo",
        CopyError::IdentityChanged => "IdentityChanged",
    }
}

fn wire_name(code: &WireErrorCode) -> &'static str {
    match code {
        WireErrorCode::Unsupported => "Unsupported",
        WireErrorCode::Invalid => "Invalid",
        WireErrorCode::Denied => "Denied",
        WireErrorCode::Conflict => "Conflict",
        WireErrorCode::Gap => "Gap",
        WireErrorCode::Expired => "Expired",
        WireErrorCode::Capacity => "Capacity",
        WireErrorCode::Uncertain => "Uncertain",
        WireErrorCode::NotFound => "NotFound",
        WireErrorCode::Closed => "Closed",
    }
}

fn via_question_mark() -> Result<u32> {
    Err(std::io::Error::other("sentinel opaque provider data"))?;
    Ok(0)
}

fn observe(error: Error) {
    let name = category_name(&error);
    let label = match &error {
        Error::Copy(code) => copy_name(code),
        Error::Stream(code) => wire_name(code),
        _ => "_",
    };
    let source = error.source().map(ToString::to_string);
    let result: marmish_native::Result<u32> = Err(error);
    let error: marmish_native::Error = result.unwrap_err();
    let plain = error.to_string();
    let padded = format!("{error:>40.6}");
    let debug = format!("{error:?}");
    if name == "Io" {
        let underlying = error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(underlying.kind(), std::io::ErrorKind::Other);
        assert!(!plain.contains("sentinel opaque provider data"));
        assert!(!padded.contains("sentinel opaque provider data"));
    } else {
        assert_eq!(source, None);
    }
    println!("{name}|{label}|{plain:?}|{padded:?}|{debug}|{source:?}");
}

fn main() {
    let ok: Result<u32> = Ok(42);
    assert!(matches!(ok, Ok(42)));
    for error in [
        Error::DaemonStartup("remote startup diagnostic: é".to_owned()),
        via_question_mark().unwrap_err(),
        Error::Invalid("malformed request"),
        Error::Identity("account missing"),
        Error::Storage("out of space"),
        Error::Transport("broken pipe"),
        Error::Mdk("unavailable"),
        Error::Timeout,
    ] {
        observe(error);
    }
    for code in [
        CopyError::Unsupported,
        CopyError::Protocol,
        CopyError::PermissionDenied,
        CopyError::NotFound,
        CopyError::UnsafePath,
        CopyError::Busy,
        CopyError::Conflict,
        CopyError::SourceChanged,
        CopyError::HashMismatch,
        CopyError::DiskFull,
        CopyError::Quota,
        CopyError::CommitUncertain,
        CopyError::LocalIo,
        CopyError::IdentityChanged,
    ] {
        observe(Error::Copy(code));
    }
    for code in [
        WireErrorCode::Unsupported,
        WireErrorCode::Invalid,
        WireErrorCode::Denied,
        WireErrorCode::Conflict,
        WireErrorCode::Gap,
        WireErrorCode::Expired,
        WireErrorCode::Capacity,
        WireErrorCode::Uncertain,
        WireErrorCode::NotFound,
        WireErrorCode::Closed,
    ] {
        observe(Error::Stream(code));
    }
}
