use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::Path,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn inputs(root: &Path) -> Result<Vec<u8>> {
    let client = "11".repeat(32);
    let server = "22".repeat(32);
    let session = "44".repeat(16);
    let mut seen = BTreeSet::new();
    let mut expected = vec![];
    let mut stdin = fs::File::create(root.join("frames"))?;
    let packets: Vec<String> = serde_json::from_str(include_str!("packets.json"))?;
    for raw in packets {
        let frame: Value = serde_json::from_str(&raw)?;
        let (role, local, peer) = match frame["type"].as_str() {
            Some("response") => ("client", &client, &server),
            Some("request") => ("server", &server, &client),
            _ => return Err("unknown packet type".into()),
        };
        let op = frame["op"].as_str().ok_or("missing operation")?;
        if !matches!(op, "cancel" | "finish" | "poll" | "status" | "submit")
            || !seen.insert((role, op.to_owned()))
            || frame["client"] != client
            || frame["server"] != server
            || frame["session"] != session
        {
            return Err("unexpected room fixture".into());
        }
        let wrong = "99".repeat(32);
        let wrong_session = "00".repeat(16);
        for (index, (c, s, id)) in [
            (&client, &server, &session),
            (&server, &client, &session),
            (&wrong, &server, &session),
            (&client, &wrong, &session),
            (&client, &server, &wrong_session),
        ]
        .into_iter()
        .enumerate()
        {
            let mut packet = frame.clone();
            packet["client"] = json!(c);
            packet["server"] = json!(s);
            packet["session"] = json!(id);
            let bytes = serde_json::to_vec(&packet)?;
            if index == 0 && bytes != raw.as_bytes() {
                return Err("canonical packet changed".into());
            }
            use std::fmt::Write;
            let mut wire = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                write!(wire, "{byte:02x}")?;
            }
            serde_json::to_writer(&mut stdin, &json!([role, op, wire, local, peer, session]))?;
            std::io::Write::write_all(&mut stdin, b"\n")?;
            expected.extend_from_slice(if index == 0 {
                b"ok\n"
            } else {
                b"command_wire_binding\n"
            });
        }
    }
    if seen.len() != 10 {
        return Err("incomplete operation coverage".into());
    }
    Ok(expected)
}

fn main() -> Result<()> {
    let [arg]: [_; 1] = std::env::args_os()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "usage: check-command-room-binding COMPILER")?;
    let compiler = fs::canonicalize(arg)?;
    let whole = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let portable = whole.join("../../oddlang/portable");
    let scratch = tempfile::tempdir()?;
    let root = scratch.path();
    for name in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(whole.join("command-requests").join(name), root.join(name))?;
    }
    fs::copy(
        whole.join("command-room-binding/observer.rs"),
        root.join("observer.rs"),
    )?;
    let mut locks = json!({});
    for (role, kind, files) in [
        (
            "server",
            "wire-protocol",
            &[
                "command-requests-v1/shape.json",
                "command-requests-v1/shape.rust.json",
                "command-requests-v1/graph.json",
                "command-requests-v1/rust.json",
            ][..],
        ),
        (
            "client",
            "wire-protocol",
            &[
                "command-response-shapes-v1/graph.json",
                "command-response-shapes-v1/rust.json",
                "command-responses-v1/graph.json",
                "command-responses-v1/rust.json",
            ],
        ),
        (
            "claim",
            "wire-schema",
            &[
                "command-room-binding-v1/shape.json",
                "command-room-binding-v1/shape.rust.json",
            ],
        ),
        (
            "relations",
            "field-relation",
            &[
                "command-room-binding-v1/graph.json",
                "command-room-binding-v1/rust.json",
            ],
        ),
    ] {
        let dir = root.join(role);
        let status = Command::new(&compiler)
            .args(["build", kind, "rust"])
            .args(files.iter().map(|f| portable.join(f)))
            .arg(&dir)
            .status()?;
        if !status.success() {
            return Err(format!("projection failed for {role}").into());
        }
        locks[role] = json!(sha(&fs::read(dir.join("projection.lock"))?));
    }
    let expected = inputs(root)?;
    let observed = Command::new("cargo")
        .args(["+1.97.1", "run", "--offline", "--locked", "--quiet"])
        .current_dir(root)
        .env("RUSTFLAGS", "-Adead_code")
        .stdin(fs::File::open(root.join("frames"))?)
        .stderr(Stdio::inherit())
        .output()?;
    if !observed.status.success() || observed.stdout != expected {
        return Err("room binding observations differ".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"compilerSha256":sha(&fs::read(compiler)?),"accepted":10,"wrongBindingRejected":40,"operationsPerRole":5,"observationSha256":sha(&observed.stdout),"projectionLocks":locks})
        )?
    );
    Ok(())
}
