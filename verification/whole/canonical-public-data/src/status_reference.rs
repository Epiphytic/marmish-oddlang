//! Compare local status observations with the archived independent expectations.
use crate::{Result, assembly::ORIGINAL, capture, load, require, sha, status_peer};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek},
    net::TcpListener,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn cases() -> Result<Vec<(&'static str, Vec<u8>)>> {
    let minimal = br#"{"ok":true}"#;
    let mut padded = minimal.to_vec();
    padded.resize(65536, b' ');
    let mut rows = Vec::new();
    for (name, body) in [
        ("defaults", minimal.as_slice()),
        ("actor-failure", br#"{"error":"busy","ok":false}"#),
        (
            "null-extensions",
            br#"{"stream":null,"command":null,"ok":true}"#,
        ),
        ("object-order", br#"{"result":{"z":1,"a":[3]},"ok":true}"#),
        ("unknown-field", br#"{"ok":true,"other":1}"#),
        ("missing-success", b"{}"),
        ("wrong-success-type", br#"{"ok":1}"#),
        ("duplicate-success", br#"{"ok":true,"ok":true}"#),
        ("malformed", b"{"),
        ("max-frame", &padded),
    ] {
        rows.push((name, status_peer::frame(body)?));
    }
    let mut extra = status_peer::frame(minimal)?;
    extra.extend(status_peer::frame(b"{}")?);
    rows.extend([
        ("zero-frame", vec![0; 4]),
        ("over-limit", 65537u32.to_be_bytes().to_vec()),
        ("no-header", vec![]),
        ("partial-header", vec![0; 2]),
        ("short-body", b"\0\0\0\x08{}".to_vec()),
        ("extra-frame", extra),
    ]);
    Ok(rows)
}

fn local(binary: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(binary);
    cmd.current_dir(home)
        .env_clear()
        .env("LANG", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("TMPDIR", home.join("tmp"))
        .stdin(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    cmd
}

fn query(binary: &Path, home: &Path, socket: &Path) -> Result<Value> {
    let out = capture(
        local(binary, home)
            .arg("--state-dir")
            .arg(home.join("state"))
            .arg("--socket")
            .arg(socket)
            .arg("status"),
        5,
        None,
    )?;
    Ok(
        json!({"exit":out.status.code(), "stdout":String::from_utf8(out.stdout)?, "stderr":String::from_utf8(out.stderr)?}),
    )
}

// Own the foreground process group until the shutdown observation has completed.
struct Owner(Child);
impl Drop for Owner {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGKILL);
            }
            let _ = self.0.wait();
        }
    }
}

fn endpoint(runtime: &Path) -> Result<Option<PathBuf>> {
    let mut sockets = Vec::new();
    for dir in fs::read_dir(runtime)? {
        let dir = dir?;
        if dir.file_name().to_string_lossy().starts_with("mr2-") {
            let path = dir.path().join("s");
            match fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_socket() => sockets.push(path),
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
    }
    Ok(if sockets.len() == 1 {
        sockets.pop()
    } else {
        None
    })
}

fn daemon(
    daemon: &Path,
    clients: &[PathBuf],
    home: &Path,
    expected: &Value,
) -> Result<(Value, Value)> {
    let relay = TcpListener::bind(("127.0.0.1", 0))?;
    let mut output = tempfile::tempfile()?;
    let mut owner = Owner(
        local(daemon, home)
            .process_group(0)
            .args(["--foreground", "--state-dir"])
            .arg(home.join("state"))
            .args(["--key-storage", "file", "--tor", "off", "--relay"])
            .arg(format!("wss://{}", relay.local_addr()?))
            .stdout(output.try_clone()?)
            .stderr(tempfile::tempfile()?)
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let socket = loop {
        if let Some(path) = endpoint(&home.join("tmp"))? {
            break path;
        }
        require(
            owner.0.try_wait()?.is_none() && Instant::now() < deadline,
            "daemon failed to expose its private status socket",
        )?;
        thread::sleep(Duration::from_millis(10));
    };
    let mut observed = Value::Null;
    for client in clients {
        observed = query(client, home, &socket)?;
        require(observed == *expected, "activating status differs")?;
    }
    require(
        unsafe { libc::kill(owner.0.id() as i32, libc::SIGTERM) } == 0,
        "failed to terminate foreground daemon",
    )?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = owner.0.try_wait()? {
            require(status.success(), "foreground shutdown failed")?;
            break;
        }
        require(Instant::now() < deadline, "foreground shutdown timed out")?;
        thread::sleep(Duration::from_millis(10));
    }
    output.rewind()?;
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes)?;
    let report: Value = serde_json::from_slice(&bytes)?;
    let mut shutdown = json!({});
    for key in [
        "engine_stop_complete",
        "mdk_stop_complete",
        "runtime_cleanup_complete",
    ] {
        require(report[key] == true, format!("shutdown incomplete: {key}"))?;
        shutdown[key] = report[key].clone();
    }
    Ok((observed, shutdown))
}

pub fn observe(bins: &Path, candidate: Option<&Path>, receipt: &Path) -> Result<Value> {
    let bins = bins.canonicalize()?;
    let build = load(receipt)?;
    require(build["commit"] == ORIGINAL, "wrong original revision")?;
    let hashes = &build["builds"]["default"]["sha256"];
    let roles = [
        "marmish",
        "marmishd",
        "marmish-engine-worker",
        "marmish-engine-bridge",
        "marmish-worker",
        "marmish-release-tool",
        "marmish-keychain",
    ];
    require(
        hashes.as_object().is_some_and(|m| m.len() == roles.len()),
        "expected seven release binaries",
    )?;
    for role in roles {
        require(
            hashes[role] == sha(&bins.join(role))?,
            format!("wrong original binary: {role}"),
        )?;
    }
    let mut clients = vec![bins.join("marmish")];
    let mut data = json!({"originalRevision":ORIGINAL,"originalBinaries":hashes,"mode":"original"});
    if let Some(candidate) = candidate {
        let path = candidate.canonicalize()?;
        data["candidateSha256"] = sha(&path)?.into();
        data["mode"] = "original-and-candidate".into();
        clients.push(path);
    }
    let root = tempfile::Builder::new().prefix("ods-").tempdir_in("/tmp")?;
    let home = root.path().canonicalize()?;
    for name in ["state", "tmp"] {
        fs::DirBuilder::new().mode(0o700).create(home.join(name))?;
    }
    let expected: Value = serde_json::from_slice(include_bytes!("../../status/macos-arm64.json"))?;
    let (real, shutdown) = daemon(
        &bins.join("marmishd"),
        &clients,
        &home,
        &expected["realDaemon"],
    )?;
    data["realDaemon"] = real;
    data["shutdown"] = shutdown;
    let frames = cases()?;
    for client in clients {
        let mut rows = status_peer::replay_frames(&client, &frames)?;
        rows["missing-socket"] = query(&client, &home, &home.join("missing"))?;
        require(
            rows == expected["controlledCases"],
            format!(
                "controlled status observations differ for {}",
                client.display()
            ),
        )?;
        data["controlledCases"] = rows;
    }
    let request = status_peer::frame(br#"{"operation":"status"}"#)?;
    data["expectedRequestHex"] = request
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into();
    data.sort_all_objects();
    Ok(data)
}
