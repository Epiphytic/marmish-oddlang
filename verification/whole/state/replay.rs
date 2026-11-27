//! Capture and reopen a disposable v1 bridge store using independent reply fixtures.
use clap::{Parser, ValueEnum};
use public_data_qualification::{
    Result, ascii_json, json_bytes, load, product::file_digests, require, sha, whole,
};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, ValueEnum)]
enum Mode {
    Capture,
    Read,
}

#[derive(Parser)]
struct Args {
    mode: Mode,
    bridge: PathBuf,
    worker: PathBuf,
    capsule: PathBuf,
    #[arg(long,default_value_os_t=whole().join("bridge/aarch64-apple-darwin.json"))]
    writer_receipt: PathBuf,
    #[arg(long,default_value_os_t=whole().join("state/aarch64-apple-darwin.json"))]
    state_receipt: PathBuf,
}

struct Pair {
    bridge: PathBuf,
    worker: PathBuf,
}

impl Pair {
    fn session(&self, root: &Path) -> Result<Value> {
        let root = root.canonicalize()?;
        let cwd = root.join("cwd");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&cwd)?;
        let mut input = Vec::new();
        for mut row in [
            json!({"op":"init","version":1,"state_root":root.join("state"),"cwd":cwd,"worker_executable":self.worker,"server":"1".repeat(64),"allowed_principals":["2".repeat(64)]}),
            json!({"op":"maintain","limit":0}),
            json!({"op":"maintain","limit":1}),
            json!({"op":"cleanup_ready"}),
        ] {
            row["id"] = "1234567890abcdef1234567890abcdef".into();
            input.extend(ascii_json(&serde_json::to_string(&row)?));
            input.push(b'\n');
        }
        let mut cmd = Command::new(&self.bridge);
        cmd.current_dir(cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C");
        // Only async-signal-safe system calls run in the child after fork.
        unsafe {
            cmd.pre_exec(|| {
                libc::umask(0o077);
                let limits = libc::rlimit {
                    rlim_cur: 256,
                    rlim_max: 256,
                };
                if libc::setrlimit(libc::RLIMIT_NOFILE, &limits) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // The bridge validates its descriptors. Its stdio must be pipes even for a local replay.
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let mut stdin = child.stdin.take().ok_or("missing bridge stdin")?;
        let stdout = child.stdout.take().ok_or("missing bridge stdout")?;
        let stderr = child.stderr.take().ok_or("missing bridge stderr")?;
        let writer = thread::spawn(move || stdin.write_all(&input));
        let reader = thread::spawn(move || std::io::read_to_string(stdout));
        let errors = thread::spawn(move || std::io::read_to_string(stderr));
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if writer.is_finished()
                && reader.is_finished()
                && errors.is_finished()
                && let Some(status) = child.try_wait()?
            {
                break status;
            }
            if Instant::now() >= deadline {
                // The child owns the process group established above.
                unsafe {
                    libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
                }
                child.wait()?;
                return Err("bridge session timed out".into());
            }
            thread::sleep(Duration::from_millis(5));
        };
        writer.join().map_err(|_| "bridge input thread failed")??;
        Ok(
            json!({"exit":status.code(),"stdout":reader.join().map_err(|_| "bridge stdout thread failed")??,
                  "stderr":errors.join().map_err(|_| "bridge stderr thread failed")??}),
        )
    }
}

// The original manifest digest covers the sorted Python JSON bytes, including separator spaces.
fn write_manifest(capsule: &Path) -> Result {
    let mut pairs = Vec::new();
    for (name, digest) in file_digests(&capsule.join("state"))? {
        pairs.push(format!(
            "{}: {}",
            serde_json::to_string(&name)?,
            serde_json::to_string(&digest)?
        ));
    }
    fs::write(
        capsule.join("manifest.json"),
        ascii_json(&format!("{{{}}}", pairs.join(", "))),
    )?;
    Ok(())
}

fn reopen(pair: &Pair, capsule: &Path, expected: &Value, corrupt: &Value) -> Result {
    let manifest = load(&capsule.join("manifest.json"))?;
    let state = capsule.join("state");
    require(
        json!(file_digests(&state)?) == manifest,
        "private input state differs from its captured manifest",
    )?;
    let copy = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let copied = copy.path().join("state");
    let status = Command::new("/bin/cp")
        .arg("-pR")
        .arg(state)
        .arg(&copied)
        .status()?;
    require(
        status.success() && json!(file_digests(&copied)?) == manifest,
        "state copy differs from manifest",
    )?;
    let observed = pair.session(copy.path())?;
    require(
        observed == *expected && sha(&copied.join("engine.key"))? == manifest["engine.key"],
        "reader failed to reopen state with identical replies and key",
    )?;
    fs::write(copied.join("engine.sqlite3"), b"not a database")?;
    require(
        pair.session(copy.path())? == json!({"exit":0,"stderr":"","stdout":corrupt["stdout"]}),
        "corrupt-state control differs",
    )
}

fn main() -> Result {
    let args = Args::parse();
    let pair = Pair {
        bridge: args.bridge.canonicalize()?,
        worker: args.worker.canonicalize()?,
    };
    let bridge = sha(&pair.bridge)?;
    let worker = sha(&pair.worker)?;
    let fixture = whole().join("state/v1.json");
    let bad = fixture.with_file_name("corrupt.json");
    let expected = load(&fixture)?;
    let corrupt = load(&bad)?;
    let path = std::path::absolute(args.capsule)?;
    let manifest = match args.mode {
        Mode::Capture => {
            let writer = load(&args.writer_receipt)?;
            let pins = &writer["builds"]["default"]["sha256"];
            require(
                pins["marmish-engine-bridge"] == bridge && pins["marmish-engine-worker"] == worker,
                "state writer binaries differ from original receipt",
            )?;
            require(!path.try_exists()?, "capture destination exists")?;
            let parent = path.parent().ok_or("missing capsule parent")?;
            fs::create_dir_all(parent)?;
            let stage = tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir_in(parent)?;
            require(
                pair.session(stage.path())? == expected,
                "original state writer differed from frozen replies",
            )?;
            write_manifest(stage.path())?;
            reopen(&pair, stage.path(), &expected, &corrupt)?;
            let digest = sha(&stage.path().join("manifest.json"))?;
            renameat_with(CWD, stage.path(), CWD, &path, RenameFlags::NOREPLACE)?;
            digest
        }
        Mode::Read => {
            let digest = sha(&path.join("manifest.json"))?;
            require(
                load(&args.state_receipt)?["privateManifestSha256"] == digest,
                "state capsule differs from selected original receipt",
            )?;
            reopen(&pair, &path, &expected, &corrupt)?;
            digest
        }
    };
    let receipt = json!({"schema":1,"bridgeSha256":bridge,"workerSha256":worker,"privateManifestSha256":manifest,
        "fixtureSha256":sha(&fixture)?,"corruptFixtureSha256":sha(&bad)?,"keyRetained":true,"reopened":true,"corruptStateRejected":true});
    std::io::stdout().lock().write_all(&json_bytes(&receipt)?)?;
    Ok(())
}
