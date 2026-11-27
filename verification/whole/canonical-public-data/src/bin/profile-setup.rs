use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, seatbelt_policy},
    capture, load, require, sha, text, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    candidate: PathBuf,
    #[arg(long)]
    candidate_build: PathBuf,
}

struct Peer {
    root: PathBuf,
    policy: String,
    role: &'static str,
}

impl Peer {
    fn run(&self, executable: &Path, args: &[&str]) -> Result<Output> {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .args(["-p", &self.policy])
            .arg(executable)
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("HOME", &self.root)
            .env("TMPDIR", self.root.join("tmp"))
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin");
        unsafe {
            command.pre_exec(|| {
                libc::umask(0o077);
                Ok(())
            });
        }
        capture(&mut command, 45, None)
    }

    fn new(
        parent: &Path,
        role: &'static str,
        installation: &Path,
        pins: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let root = parent.join(role);
        fs::create_dir(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        fs::create_dir(root.join("tmp"))?;
        let mut allowed = vec![
            PathBuf::from("/usr/bin/nc"),
            PathBuf::from("/usr/bin/security"),
            PathBuf::from("/bin/cat"),
            PathBuf::from("/usr/bin/touch"),
        ];
        for (name, expected) in pins {
            require(
                Path::new(name).file_name() == Some(name.as_ref()),
                "invalid pinned executable name",
            )?;
            let source = installation.join(name);
            let metadata = fs::symlink_metadata(&source)?;
            require(
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
                format!("missing executable {name}"),
            )?;
            let dest = root.join(name);
            fs::copy(source, &dest)?;
            require(
                sha(&dest)? == *expected,
                format!("binary digest mismatch for {name}"),
            )?;
            allowed.push(dest);
        }
        let filters = allowed
            .iter()
            .map(|p| {
                Ok(format!(
                    "(literal {})",
                    serde_json::to_string(&p.canonicalize()?)?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(" ");
        let sentinel = parent.join("sentinel");
        let mut policy = seatbelt_policy(&root, &[], &[&sentinel])?;
        policy.push_str(&format!(" (deny mach-lookup) (allow mach-lookup (global-name \"com.apple.system.opendirectoryd.libinfo\")) (deny process-exec) (allow process-exec {filters}) (deny file-write* {filters})"));
        let peer = Self { root, policy, role };
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port().to_string();
        let socket_args = ["-v", "-z", "-w", "1", "127.0.0.1", &port];
        require(
            Command::new("/usr/bin/nc")
                .args(socket_args)
                .output()?
                .status
                .success(),
            "loopback listener unavailable",
        )?;
        let denied = peer.run(Path::new("/usr/bin/nc"), &socket_args)?;
        let diagnostic = String::from_utf8(denied.stderr)?;
        require(
            !denied.status.success()
                && diagnostic.contains("127.0.0.1")
                && diagnostic.contains("Operation not permitted"),
            format!("missing socket denial: {diagnostic}"),
        )?;
        let inside = peer.root.join("write-probe");
        require(
            peer.run(
                Path::new("/usr/bin/touch"),
                &[inside.to_str().ok_or("invalid probe path")?],
            )?
            .status
            .success()
                && inside.exists(),
            "private write failed",
        )?;
        require(
            !peer
                .run(
                    Path::new("/bin/cat"),
                    &[sentinel.to_str().ok_or("invalid sentinel path")?],
                )?
                .status
                .success(),
            "external read allowed",
        )?;
        require(
            !peer
                .run(
                    Path::new("/usr/bin/touch"),
                    &[sentinel.to_str().ok_or("invalid sentinel path")?],
                )?
                .status
                .success(),
            "external write allowed",
        )?;
        require(
            !peer
                .run(
                    Path::new("/usr/bin/security"),
                    &["default-keychain", "-d", "user"],
                )?
                .status
                .success(),
            "keychain lookup allowed",
        )?;
        Ok(peer)
    }
}

#[derive(Default)]
struct Cases {
    ids: BTreeMap<&'static str, String>,
    keys: BTreeMap<&'static str, String>,
    rows: serde_json::Map<String, Value>,
}

impl Cases {
    fn invoke(
        &mut self,
        peer: &Peer,
        relay: &str,
        case: &str,
        operation: &str,
        options: &[&str],
        status: i32,
    ) -> Result {
        let role = peer.role;
        let state = peer.root.join("state");
        let mut args = vec![
            "--state-dir",
            state.to_str().ok_or("invalid state path")?,
            "--identity",
            role,
            "--key-storage",
            "file",
            "--tor",
            "auto",
            "--relay",
            relay,
            operation,
        ];
        args.extend(options);
        let output = peer.run(&peer.root.join("marmish"), &args)?;
        require(
            output.status.code() == Some(status),
            format!("{case}: expected {status}, got {}", output.status),
        )?;
        let mut stdout = String::from_utf8(output.stdout)?;
        let profile = state.join(role);
        if status == 0 {
            let report: Value = serde_json::from_str(&stdout)?;
            let manifest = load(&profile.join("profile.json"))?;
            let key = text(&manifest["public_key"])?;
            require(
                key.len() == 64
                    && key.bytes().all(|b| b.is_ascii_hexdigit())
                    && self.keys.entry(role).or_insert(key.into()) == key,
                "signer changed",
            )?;
            if operation == "init" {
                require(
                    report["identity"] == key,
                    "init reopened a different signer",
                )?;
            } else {
                let id = text(&report["npub"])?;
                require(
                    !id.is_empty()
                        && self.ids.iter().all(|(r, v)| *r == role || v != id)
                        && self.ids.entry(role).or_insert(id.into()) == id,
                    "unexpected peer identity",
                )?;
            }
        }
        let file = profile.join("config.json");
        let mode = fs::metadata(&file)?.permissions().mode() & 0o777;
        require(mode == 0o600, "incorrect config permissions")?;
        let mut config = fs::read_to_string(file)?;
        for (role, id) in &self.ids {
            stdout = stdout.replace(id, &format!("<npub-{role}>"));
        }
        for (role, key) in &self.keys {
            let label = format!("<hex-{role}>");
            stdout = stdout.replace(key, &label);
            config = config.replace(key, &label);
        }
        self.rows.insert(case.into(),json!({"status":status,"stdout":bytes_sha(stdout.as_bytes()),"stderr":bytes_sha(&output.stderr),"config":bytes_sha(config.as_bytes()),"configMode":mode}));
        Ok(())
    }
}

fn observe(installation: &Path, pins: &BTreeMap<String, String>, relay: &str) -> Result<Value> {
    let work = tempfile::tempdir()?;
    fs::set_permissions(work.path(), fs::Permissions::from_mode(0o700))?;
    let parent = work.path().canonicalize()?;
    fs::write(parent.join("sentinel"), b"external read probe")?;
    let client = Peer::new(&parent, "client", installation, pins)?;
    let server = Peer::new(&parent, "server", installation, pins)?;
    let mut cases = Cases::default();
    for peer in [&client, &server] {
        cases.invoke(
            peer,
            relay,
            &format!("{}-fresh", peer.role),
            "setup",
            &["--role", peer.role, "--no-grants"],
            0,
        )?;
    }
    let client_id = cases.ids["client"].clone();
    let server_id = cases.ids["server"].clone();
    for (peer, option, id) in [
        (&server, "--grant", &client_id),
        (&client, "--trust", &server_id),
    ] {
        for (suffix, key, status) in [
            ("update", id.as_str(), 0),
            ("repeat", id, 0),
            ("invalid", "bad-public-key", 1),
        ] {
            cases.invoke(
                peer,
                relay,
                &format!("{}-{suffix}", peer.role),
                "setup",
                &["--update", option, key],
                status,
            )?;
        }
        require(
            cases.rows[&format!("{}-invalid", peer.role)]["config"]
                == cases.rows[&format!("{}-update", peer.role)]["config"],
            "rejection changed authorization",
        )?;
    }
    cases.invoke(&server, relay, "init-with-grant", "init", &[], 0)?;
    for (case, options, status) in [
        (
            "retain-grant-while-client",
            vec!["--update", "--role", "client"],
            0,
        ),
        (
            "deny-new-client-grant",
            vec!["--update", "--grant", &client_id],
            1,
        ),
        ("reactivate-grant", vec!["--update", "--role", "server"], 0),
        ("implicit-revoke", vec!["--update", "--no-grants"], 1),
        ("revoke", vec!["--update", "--revoke", &client_id], 0),
    ] {
        cases.invoke(&server, relay, case, "setup", &options, status)?;
    }
    cases.invoke(
        &client,
        relay,
        "untrust",
        "setup",
        &["--update", "--untrust", &server_id],
        0,
    )?;
    work.close()?;
    require(!parent.exists(), "profiles survived cleanup")?;
    Ok(Value::Object(cases.rows))
}

fn main() -> Result {
    let args = Args::parse();
    let record =
        whole().join("basic-command-interop/upstream-owner-activation-blocked-macos-arm64.json");
    let reference = load(&record)?;
    let build = load(&args.candidate_build)?;
    require(
        build["format"] == "marmish-basic-command-generated-build-v1"
            && text(&reference["upstream"])? == text(&build["referenceTargetRevision"])?,
        "reference revision mismatch",
    )?;
    let originals = serde_json::from_value(reference["reference_binary_sha256"].clone())?;
    let mut candidates = BTreeMap::new();
    for (name, row) in build["artifacts"]
        .as_object()
        .ok_or("missing build artifacts")?
    {
        candidates.insert(name.clone(), text(&row["sha256"])?.to_owned());
    }
    require(
        candidates.contains_key("marmish"),
        "missing pinned candidate CLI",
    )?;
    let mut cases = json!({});
    for (route, relay) in [
        ("wss", "wss://127.0.0.1:9443"),
        (
            "onion",
            "ws://aaaqeayeaudaocajbifqydiob4ibceqtcqkrmfyydenbwha5dyp3kead.onion/",
        ),
    ] {
        let expected = observe(&args.original, &originals, relay)?;
        let actual = observe(&args.candidate, &candidates, relay)?;
        require(
            actual == expected,
            format!("{route} profile results differ"),
        )?;
        cases[route] = actual;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"cases":cases,"workspacesRemoved":true,"networkDenied":true,"candidateBuildSha256":sha(&args.candidate_build)?,"referenceRecordSha256":sha(&record)?,"originalSha256":originals["marmish"],"candidateSha256":candidates["marmish"]})
        )?
    );
    Ok(())
}
