use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, seatbelt_policy},
    capture, load, portable,
    product::check_members,
    require, run, sha, text, whole,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    #[arg(
        long,
        required_unless_present = "command_policy_workspace",
        conflicts_with = "command_policy_workspace"
    )]
    candidate_build: Option<PathBuf>,
    /// Build the candidate from the published command-policy workspace lock.
    #[arg(long)]
    command_policy_workspace: bool,
    /// Also exercise the two mixed client/server pairings.
    #[arg(long)]
    mixed: bool,
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
            format!(
                "{case}: expected {status}, got {}, stderr {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
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

struct Installation {
    directory: PathBuf,
    pins: BTreeMap<String, String>,
}

fn file_keystore(peer: &Peer) -> Result {
    let profile = peer.root.join("state").join(peer.role);
    let manifest = load(&profile.join("profile.json"))?;
    require(
        manifest["backend"] == "files" && manifest["phase"] == "ready",
        "file-backed profile is not ready",
    )?;
    let keys = profile.join("keys");
    let meta = fs::symlink_metadata(&keys)?;
    require(
        meta.is_dir() && meta.permissions().mode() & 0o777 == 0o700,
        "unexpected key directory mode",
    )?;
    let mut count = 0;
    for entry in fs::read_dir(keys)? {
        let meta = fs::symlink_metadata(entry?.path())?;
        require(
            meta.is_file() && meta.permissions().mode() & 0o777 == 0o600,
            "unexpected credential file",
        )?;
        count += 1;
    }
    require(count == 2, "expected two private credential files")
}

fn clean_daemon_census() -> Result {
    let out = Command::new("/usr/bin/pgrep")
        .args(["-x", "marmishd"])
        .output()?;
    require(
        out.status.code() == Some(1) && out.stdout.is_empty() && out.stderr.is_empty(),
        "expected zero active marmishd processes",
    )
}

fn observe(
    client_install: &Installation,
    server_install: &Installation,
    relay: &str,
    identities: &mut BTreeSet<String>,
    check_allowlist: bool,
) -> Result<Value> {
    let work = tempfile::tempdir()?;
    fs::set_permissions(work.path(), fs::Permissions::from_mode(0o700))?;
    let parent = work.path().canonicalize()?;
    fs::write(parent.join("sentinel"), b"external read probe")?;
    let client = Peer::new(
        &parent,
        "client",
        &client_install.directory,
        &client_install.pins,
    )?;
    let server = Peer::new(
        &parent,
        "server",
        &server_install.directory,
        &server_install.pins,
    )?;
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
    for peer in [&client, &server] {
        file_keystore(peer)?;
        require(
            identities.insert(cases.keys[peer.role].clone()),
            "reused disposable identity",
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
        let config = load(&peer.root.join("state").join(peer.role).join("config.json"))?;
        let (clients, remotes) = if peer.role == "server" {
            (json!([cases.keys["client"]]), json!([]))
        } else {
            (json!([]), json!([cases.keys["server"]]))
        };
        require(
            config["execution_clients"] == clients
                && config["trusted_remotes"] == remotes
                && config["relays"] == json!([relay]),
            "peer authorization differs",
        )?;
        require(
            cases.rows[&format!("{}-invalid", peer.role)]["config"]
                == cases.rows[&format!("{}-update", peer.role)]["config"],
            "rejection changed authorization",
        )?;
    }
    cases.invoke(&server, relay, "init-with-grant", "init", &[], 0)?;
    if check_allowlist {
        let path = server.root.join("state/server/config.json");
        let mut config = load(&path)?;
        for (case, policy, diagnostic) in [
            (
                "init-null-allowlist",
                json!({"version":1,"defaults":{"allow_exec":null}}),
                "config_schema",
            ),
            (
                "init-unsupported-version",
                json!({"version":2}),
                "config_policy_bounds",
            ),
            (
                "init-invalid-principal",
                json!({"version":1,"keys":[{"principal":"invalid","restrictions":{}}]}),
                "config_account",
            ),
        ] {
            let mut invalid = config.clone();
            invalid["command_policy"] = policy;
            let bytes = serde_json::to_vec(&invalid)?;
            fs::write(&path, &bytes)?;
            cases.invoke(&server, relay, case, "init", &[], 1)?;
            require(
                fs::read(&path)? == bytes
                    && cases.rows[case]["stdout"] == bytes_sha(b"")
                    && cases.rows[case]["stderr"]
                        == bytes_sha(format!("marmish: invalid input: {diagnostic}\n").as_bytes()),
                format!("{case}: rejection differs"),
            )?;
        }
        config["command_policy"]["defaults"] = json!({"allow_exec":["/usr/bin/true"]});
        fs::write(&path, serde_json::to_vec(&config)?)?;
        cases.invoke(&server, relay, "init-with-allowlist", "init", &[], 0)?;
        require(
            load(&path)? == config,
            "reopening changed the authorized server configuration",
        )?;
    }
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
    let originals = Installation {
        directory: args.original,
        pins: serde_json::from_value(reference["reference_binary_sha256"].clone())?,
    };
    let build_target = tempfile::tempdir()?;
    let (candidate, provenance) = if let Some(path) = args.candidate_build {
        let build = load(&path)?;
        require(
            build["format"] == "marmish-basic-command-generated-build-v1"
                && text(&reference["upstream"])? == text(&build["referenceTargetRevision"])?,
            "reference revision mismatch",
        )?;
        let mut pins = BTreeMap::new();
        for (name, row) in build["artifacts"]
            .as_object()
            .ok_or("missing build artifacts")?
        {
            pins.insert(name.clone(), text(&row["sha256"])?.to_owned());
        }
        require(pins.contains_key("marmish"), "missing pinned candidate CLI")?;
        (
            Installation {
                directory: args.candidate,
                pins,
            },
            ("candidateBuildSha256", sha(&path)?),
        )
    } else {
        let workspace = args.candidate.canonicalize()?;
        let lock = workspace.join("workspace.lock");
        require(
            fs::read(&lock)?
                == fs::read(portable().join("workspace-profile-command-policy/workspace.lock"))?,
            "command-policy workspace lock differs",
        )?;
        check_members(&workspace, &load(&lock)?, true)?;
        run(Command::new("cargo")
            .current_dir(&workspace)
            .args([
                "+1.97.1",
                "build",
                "--workspace",
                "--bins",
                "--release",
                "--offline",
                "--locked",
                "--target-dir",
            ])
            .arg(build_target.path())
            .env_remove("CARGO_BUILD_TARGET"))?;
        let directory = build_target.path().join("release");
        let pins = ["marmish", "marmishd"]
            .into_iter()
            .map(|n| Ok((n.to_string(), sha(&directory.join(n))?)))
            .collect::<Result<_>>()?;
        (
            Installation { directory, pins },
            ("candidateWorkspaceLockSha256", sha(&lock)?),
        )
    };
    let mut identities = BTreeSet::new();
    let mut pair = |client: &Installation, server: &Installation, relay: &str| -> Result<Value> {
        if args.mixed {
            clean_daemon_census()?;
        }
        let result = observe(client, server, relay, &mut identities, args.mixed)?;
        if args.mixed {
            clean_daemon_census()?;
        }
        Ok(result)
    };
    let mut mixed = json!({});
    let mut cases = json!({});
    for (route, relay) in [
        ("wss", "wss://127.0.0.1:9443"),
        (
            "onion",
            "ws://aaaqeayeaudaocajbifqydiob4ibceqtcqkrmfyydenbwha5dyp3kead.onion/",
        ),
    ] {
        let expected = pair(&originals, &originals, relay)?;
        let actual = pair(&candidate, &candidate, relay)?;
        require(
            actual == expected,
            format!("{route} profile results differ"),
        )?;
        if args.mixed {
            for (label, client, server) in [
                ("generatedClient", &candidate, &originals),
                ("generatedServer", &originals, &candidate),
            ] {
                let observed = pair(client, server, relay)?;
                require(
                    observed == expected,
                    format!("{route} {label} setup differs"),
                )?;
                mixed[route][label] = observed;
            }
        }
        cases[route] = actual;
    }
    let mut report = json!({"cases":cases});
    report["workspacesRemoved"] = true.into();
    report["networkDenied"] = true.into();
    report[provenance.0] = provenance.1.into();
    report["referenceRecordSha256"] = sha(&record)?.into();
    report["originalSha256"] = originals.pins["marmish"].clone().into();
    report["candidateSha256"] = candidate.pins["marmish"].clone().into();
    if args.mixed {
        report["mixed"] = mixed;
        report["distinctDisposableIdentities"] = identities.len().into();
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
