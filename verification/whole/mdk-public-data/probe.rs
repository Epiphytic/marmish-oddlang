use marmish_mdk::{AdapterError, VersionObservation};
use std::error::Error as _;

fn name(error: &AdapterError) -> &'static str {
    match error {
        AdapterError::InvalidExecutable => "InvalidExecutable",
        AdapterError::MissingExecutable => "MissingExecutable",
        AdapterError::Timeout => "Timeout",
        AdapterError::OutputLimit => "OutputLimit",
        AdapterError::InvalidResponse => "InvalidResponse",
        AdapterError::SubprocessFailure => "SubprocessFailure",
        AdapterError::LiveEgressUnverified => "LiveEgressUnverified",
    }
}

fn main() {
    for error in [
        AdapterError::InvalidExecutable,
        AdapterError::MissingExecutable,
        AdapterError::Timeout,
        AdapterError::OutputLimit,
        AdapterError::InvalidResponse,
        AdapterError::SubprocessFailure,
        AdapterError::LiveEgressUnverified,
    ] {
        let label = name(&error);
        let display = error.to_string();
        assert!(error.source().is_none());
        let debug = format!("{error:?}");
        println!("{label}|{debug}|{display}|{}", error as isize);
    }

    let mut observation = VersionObservation {
        stdout: "released \"é\"\n".to_owned(),
        stderr: "\tstatus\r\n".to_owned(),
    };
    observation.stdout.push('!');
    println!("{observation:?}");
    let VersionObservation { stdout, stderr } = observation;
    assert_eq!(stdout, "released \"é\"\n!");
    assert_eq!(stderr, "\tstatus\r\n");
}
