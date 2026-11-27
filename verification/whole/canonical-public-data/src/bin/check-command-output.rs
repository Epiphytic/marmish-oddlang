//! Compare observed CLI streams before decoding or summarizing them.
use public_data_qualification::{Result, command_output::Output, require};
use std::{
    env,
    io::{self, Read, Write},
};

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8]> {
    let (head, tail) = bytes
        .split_at_checked(count)
        .ok_or("incomplete command output")?;
    *bytes = tail;
    Ok(head)
}
fn read<'a>(bytes: &mut &'a [u8]) -> Result<Output<'a>> {
    let status = i32::from_be_bytes(take(bytes, 4)?.try_into()?);
    let out = usize::try_from(u64::from_be_bytes(take(bytes, 8)?.try_into()?))?;
    let err = usize::try_from(u64::from_be_bytes(take(bytes, 8)?.try_into()?))?;
    Ok(Output {
        status,
        stdout: take(bytes, out)?,
        stderr: take(bytes, err)?,
    })
}
fn check(args: &[String], mut input: &[u8]) -> Result<Vec<u8>> {
    let actual = read(&mut input)?;
    let output = match args {
        [mode] if mode == "receipt" => serde_json::to_vec(&actual.receipt())?,
        [mode, server] if mode == "handle" => actual.handle(server)?.to_vec(),
        [mode] if mode == "compare" => {
            actual.compare(&read(&mut input)?)?;
            Vec::new()
        }
        [mode, elapsed] if mode == "goodput" => {
            let expected = read(&mut input)?;
            let ns: u64 = elapsed
                .parse()
                .map_err(|_| "invalid finite benchmark duration")?;
            serde_json::to_vec(&actual.goodput(&expected, ns)?)?
        }
        _ => return Err("expected compare, receipt, handle SERVER, or goodput ELAPSED_NS".into()),
    };
    require(input.is_empty(), "trailing command output data")?;
    Ok(output)
}
fn run() -> Result {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;
    io::stdout().write_all(&check(&env::args().skip(1).collect::<Vec<_>>(), &input)?)?;
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(status: i32, out: &[u8], err: &[u8]) -> Vec<u8> {
        let mut bytes = status.to_be_bytes().to_vec();
        bytes.extend((out.len() as u64).to_be_bytes());
        bytes.extend((err.len() as u64).to_be_bytes());
        bytes.extend(out);
        bytes.extend(err);
        bytes
    }
    #[test]
    fn checks_both_streams_and_status() {
        let actual = frame(7, b"\xff\n\n", b"\0");
        let pair = |r: &[u8]| check(&["compare".into()], &[actual.as_slice(), r].concat());
        assert!(pair(&actual).is_ok());
        for changed in [
            frame(0, b"\xff\n\n", b"\0"),
            frame(7, b"\xff\n", b"\0"),
            frame(7, b"\xff\n\n", b""),
            frame(7, b"\0", b"\xff\n\n"),
        ] {
            assert!(pair(&changed).is_err());
        }
        for len in 0..actual.len() {
            assert!(pair(&actual[..len]).is_err());
        }
        assert!(pair(&[actual.as_slice(), b"x"].concat()).is_err());
    }
    #[test]
    fn binds_handle_to_receiver_and_exact_cli_bytes() {
        let server = "1a".repeat(32);
        let expected = format!(
            r#"{{"format":1,"profile":"client","remote":"{}","session":"{}","invocation":"{}","submit_sha256":"{}"}}"#,
            server,
            "03".repeat(16),
            "04".repeat(16),
            "ff".repeat(32)
        );
        let stderr = format!("marmish handle: {expected}\n");
        let args = ["handle".into(), server];
        let ok = frame(0, b"", stderr.as_bytes());
        assert_eq!(check(&args, &ok).unwrap(), expected.as_bytes());
        for bad in [
            frame(7, b"", stderr.as_bytes()),
            frame(0, b"\n", stderr.as_bytes()),
            frame(0, b"", stderr.trim_end().as_bytes()),
            frame(0, b"", stderr.replace("\":1", "\": 1").as_bytes()),
            frame(0, b"", format!("{stderr}extra\n").as_bytes()),
        ] {
            assert!(check(&args, &bad).is_err());
        }
        assert!(check(&["handle".into(), "ff".repeat(32)], &ok).is_err());
    }
    #[test]
    fn goodput_requires_exact_data_size_and_time() {
        let actual = frame(7, &vec![b'a'; 196608], &vec![b'b'; 65536]);
        let pair = [actual.as_slice(), &actual].concat();
        let rate = |ns: &str, input: &[u8]| check(&["goodput".into(), ns.into()], input);
        assert_eq!(
            serde_json::from_slice::<f64>(&rate("2000000000", &pair).unwrap()).unwrap(),
            131072.0
        );
        for ns in ["0", "-1", "330000000001", "true"] {
            assert!(rate(ns, &pair).is_err());
        }
        for ns in ["1", "330000000000"] {
            assert!(rate(ns, &pair).is_ok());
        }
        let corrupt = frame(7, &vec![b'c'; 196608], &vec![b'b'; 65536]);
        assert!(rate("1", &[actual, corrupt].concat()).is_err());
        assert!(rate("1", &frame(0, b"", b"").repeat(2)).is_err());
    }
}
