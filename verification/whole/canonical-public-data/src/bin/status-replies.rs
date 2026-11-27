//! Compare local status reply decoding with observations from a pinned original client.
use clap::{ArgGroup, Parser, ValueEnum};
use public_data_qualification::{
    Result, ascii_json, assembly::bytes_sha, capture, json_bytes, load, require, sha, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::DirBuilderExt,
        net::UnixListener,
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, ValueEnum)]
enum Corpus {
    Extended,
    Nested,
}
#[derive(Parser)]
#[command(group(ArgGroup::new("reference").required(true).args(["compare","reference_status"])))]
struct Args {
    corpus: Corpus,
    binary: PathBuf,
    receipt: PathBuf,
    #[arg(long)]
    reference_status: Option<PathBuf>,
    #[arg(long)]
    compare: Option<PathBuf>,
}

fn exchange(listener: &UnixListener, payload: &[u8]) -> io::Result<()> {
    let timeout = Duration::from_secs(4);
    let deadline = Instant::now() + timeout;
    let mut socket = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    };
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(timeout))?;
    socket.set_write_timeout(Some(timeout))?;
    const REQUEST: &[u8] = b"\0\0\0\x16{\"operation\":\"status\"}";
    let mut bytes = Vec::new();
    (&socket)
        .take(REQUEST.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected exactly one status request followed by write EOF",
        ));
    }
    let size = u32::try_from(payload.len()).map_err(io::Error::other)?;
    socket.write_all(&size.to_be_bytes())?;
    socket.write_all(payload)
}

fn replay(binary: &Path, cases: &Value) -> Result<Value> {
    let home = tempfile::Builder::new()
        .prefix("odshape-")
        .tempdir_in("/tmp")?;
    let state = home.path().join("state");
    fs::DirBuilder::new().mode(0o700).create(&state)?;
    let socket = home.path().join("s");
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let mut results = json!({});
    for (name, reply) in cases.as_object().ok_or("expected named cases")? {
        let mut cmd = Command::new(binary);
        cmd.arg("--state-dir")
            .arg(&state)
            .arg("--socket")
            .arg(&socket)
            .arg("status")
            .current_dir(home.path())
            .env_clear()
            .env("HOME", home.path())
            .env("TMPDIR", home.path())
            .env("LANG", "C")
            .env("PATH", "/bin:/usr/bin");
        // The child sets its private file creation mask before exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::umask(0o077);
                Ok(())
            });
        }
        let wire = ascii_json(&serde_json::to_string(reply)?);
        let output = thread::scope(|scope| -> Result<_> {
            let peer = scope.spawn(|| exchange(&listener, &wire));
            let process = capture(&mut cmd, 9, None);
            peer.join().map_err(|_| "status peer panicked")??;
            process
        })?;
        let exit = output
            .status
            .code()
            .or_else(|| output.status.signal().map(|s| -s))
            .ok_or("missing exit status")?;
        results[name] = json!({"exit":exit,"stdout":String::from_utf8(output.stdout)?,"stderr":String::from_utf8(output.stderr)?});
    }
    Ok(results)
}
fn main() -> Result<()> {
    let args = Args::parse();
    let binary = args.binary.canonicalize()?;
    let digest = sha(&binary)?;
    let file = match args.corpus {
        Corpus::Extended => "reply-shapes.json",
        Corpus::Nested => "typed-reply-variants.json",
    };
    let bytes = fs::read(whole().join("status").join(file))?;
    let hash = bytes_sha(&bytes);
    let candidate = args.compare.is_some();
    let reference = load(
        args.compare
            .as_ref()
            .or(args.reference_status.as_ref())
            .ok_or("missing reference")?,
    )?;
    require(
        reference["mode"] == "original",
        "expected original reference",
    )?;
    if candidate {
        require(reference["caseSha256"] == hash, "corpus digest differs")?;
    } else {
        require(
            reference["originalBinaries"]["marmish"] == digest,
            "original binary digest differs",
        )?;
    }
    let results = replay(&binary, &serde_json::from_slice(&bytes)?)?;
    let rows = results.as_object().ok_or("missing results")?;
    let mut receipt = json!({"binarySha256":digest,"caseSha256":hash,"mode":if candidate{"candidate"}else{"original"},"originalRevision":reference.get("originalRevision").ok_or("missing original revision")?,"results":results});
    let mut mismatches = Vec::new();
    if candidate {
        let expected = reference["results"]
            .as_object()
            .ok_or("missing reference results")?;
        require(
            expected.len() == rows.len() && rows.keys().all(|k| expected.contains_key(k)),
            "case names differ",
        )?;
        mismatches.extend(
            rows.iter()
                .filter(|(k, v)| expected[*k] != **v)
                .map(|(k, _)| k.as_str()),
        );
        mismatches.sort();
        receipt["mismatches"] = json!(mismatches);
    }
    receipt.sort_all_objects();
    fs::write(args.receipt, json_bytes(&receipt)?)?;
    require(
        mismatches.is_empty(),
        format!("different replies: {}", mismatches.join(", ")),
    )?;
    if matches!(args.corpus, Corpus::Nested) {
        println!("Observed {} typed local reply variants", rows.len());
    }
    Ok(())
}
