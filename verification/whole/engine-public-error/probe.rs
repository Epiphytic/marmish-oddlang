use marmish_engine::{EngineError as E, NotExecutableReason as X, NotFoundReason as N, ResolutionReason as R, Result};
use std::error::Error;

fn category(error: &E) -> &'static str {
    match error {
        E::Unauthorized => "Unauthorized",
        E::RootForbidden => "RootForbidden",
        E::InvalidPolicy => "InvalidPolicy",
        E::UnsafePath => "UnsafePath",
        E::AlreadyOpen => "AlreadyOpen",
        E::InvalidRequest => "InvalidRequest",
        E::CommandDenied => "CommandDenied",
        E::CommandNotFound(reason) => match reason {
            N::PathSearchExhausted | N::ExecutableMissing => "CommandNotFound",
        },
        E::CommandNotExecutable(reason) => match reason {
            X::FilesystemPermission | X::NotRegularFile | X::ExecuteBitMissing => "CommandNotExecutable",
        },
        E::CommandResolutionFailed(reason) => match reason {
            R::SymlinkLoop | R::FilesystemIo | R::InvalidSearchPath => "CommandResolutionFailed",
        },
        E::NotFound => "NotFound",
        E::InvocationConflict => "InvocationConflict",
        E::InvalidToken => "InvalidToken",
        E::ExpiredToken => "ExpiredToken",
        E::QuotaExceeded => "QuotaExceeded",
        E::StaleGeneration => "StaleGeneration",
        E::InputExpired => "InputExpired",
        E::InputConflict => "InputConflict",
        E::InputClosed => "InputClosed",
        E::InputGap => "InputGap",
        E::WorkerUnavailable => "WorkerUnavailable",
        E::RecoveryUncertain => "RecoveryUncertain",
        E::StorageFailure => "StorageFailure",
        E::UnsupportedStorage => "UnsupportedStorage",
        E::Busy => "Busy",
        E::InputBackpressure { next_offset, available_credit } => {
            let _: (&u64, &u32) = (next_offset, available_credit);
            "InputBackpressure"
        }
    }
}

fn reason<T: Clone + Copy + Eq + std::fmt::Debug + Send + Sync>(value: T, ordinal: isize) {
    assert_eq!(value, value.clone());
    println!("reason|{value:?}|{ordinal}");
}

fn main() {
    let identity: Result<&str> = Ok("alias");
    assert_eq!(identity.unwrap(), "alias");
    let initial: fn() -> Result<()> = marmish_engine::initialize_process_limits;
    let _ = initial;
    for value in [N::PathSearchExhausted, N::ExecutableMissing] {
        reason(value, value as isize);
    }
    for value in [X::FilesystemPermission, X::NotRegularFile, X::ExecuteBitMissing] {
        reason(value, value as isize);
    }
    for value in [R::SymlinkLoop, R::FilesystemIo, R::InvalidSearchPath] {
        reason(value, value as isize);
    }

    let mut rows = vec![
        E::Unauthorized, E::RootForbidden, E::InvalidPolicy, E::UnsafePath, E::AlreadyOpen,
        E::InvalidRequest, E::CommandDenied,
        E::CommandNotFound(N::PathSearchExhausted), E::CommandNotFound(N::ExecutableMissing),
        E::CommandNotExecutable(X::FilesystemPermission), E::CommandNotExecutable(X::NotRegularFile),
        E::CommandNotExecutable(X::ExecuteBitMissing),
        E::CommandResolutionFailed(R::SymlinkLoop), E::CommandResolutionFailed(R::FilesystemIo),
        E::CommandResolutionFailed(R::InvalidSearchPath), E::NotFound, E::InvocationConflict,
        E::InvalidToken, E::ExpiredToken, E::QuotaExceeded, E::StaleGeneration, E::InputExpired,
        E::InputConflict, E::InputClosed, E::InputGap, E::WorkerUnavailable, E::RecoveryUncertain,
        E::StorageFailure, E::UnsupportedStorage, E::Busy,
    ];
    for (next_offset, available_credit) in [(0, 0), (1, u32::MAX), (u64::MAX, 0), (u64::MAX, u32::MAX)] {
        rows.push(E::InputBackpressure { next_offset, available_credit });
    }
    fn capabilities<T: Eq + Error + Send + Sync + Unpin>() {}
    capabilities::<E>();
    for (index, error) in rows.iter().enumerate() {
        assert!(error.source().is_none());
        for (other, value) in rows.iter().enumerate() {
            assert_eq!(error == value, index == other);
            assert_eq!(std::mem::discriminant(error) == std::mem::discriminant(value),
                       category(error) == category(value));
        }
        println!("error|{}|{}|{:?}|{:#?}", category(error), error, error, error);
    }
}
