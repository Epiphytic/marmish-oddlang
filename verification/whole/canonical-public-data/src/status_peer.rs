use crate::{Result, ascii_json, capture};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::DirBuilderExt,
        net::UnixListener,
        process::{CommandExt, ExitStatusExt},
    },
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

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

pub fn replay(binary: &Path, cases: &Value) -> Result<Value> {
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
