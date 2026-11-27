#[path = "../command-requests/verify.rs"]
mod requests;
mod shapes;
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const OPERATIONS: [&str; 5] = ["submit", "status", "cancel", "poll", "finish"];
const REFERENCE: &[u8] = include_bytes!("reference-envelope-aarch64-apple-darwin.json");

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Requests {
        compiler: PathBuf,
        original: PathBuf,
        #[arg(long)]
        package: Option<PathBuf>,
        #[arg(long, requires = "package")]
        workspace_lock: Option<PathBuf>,
    },
    Shapes {
        compiler: PathBuf,
        original: PathBuf,
    },
    Reference {
        original: PathBuf,
    },
    Generated {
        compiler: PathBuf,
        #[arg(long, requires = "workspace_lock")]
        workspace: Option<PathBuf>,
        #[arg(long, requires = "workspace")]
        workspace_lock: Option<PathBuf>,
    },
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}
fn git_blob(repo: &Path, name: &str) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "show",
            &format!("{ORIGINAL}:crates/marmish-native/src/{name}"),
        ])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output.stdout)
}

fn workspace_evidence(workspace: &Path, pin: &Path, module: &str, source: &[u8]) -> Result<Value> {
    let lock = fs::read(workspace.join("workspace.lock"))?;
    if lock != fs::read(pin)? {
        return Err("workspace differs from pinned selection".into());
    }
    let member = workspace.join("marmish-native");
    let package = fs::read(member.join("package.lock"))?;
    let selection: Value = serde_json::from_slice(&lock)?;
    let member_hash = sha(&package);
    if selection["members"]["marmish-native"]["standalonePackageLockSha256"] != member_hash {
        return Err("native member differs from pinned workspace".into());
    }
    let manifest: Value = serde_json::from_slice(&package)?;
    let source_hash = sha(source);
    let mut selected = manifest["components"]
        .as_array()
        .ok_or("missing native components")?
        .iter()
        .filter(|c| c["module"] == module);
    let codec = selected.next().ok_or("missing selected wire component")?;
    if selected.next().is_some()
        || codec["kind"] != "wire-protocol"
        || codec["sourceSha256"] != source_hash
        || codec["projection"]["wireSha256"] != source_hash
    {
        return Err("selected wire component differs from standalone codec".into());
    }
    let paths: Vec<_> = manifest["generatedFiles"]
        .as_object()
        .ok_or("missing generated files")?
        .iter()
        .filter_map(|(k, v)| (v == &source_hash).then_some(k))
        .collect();
    let [path] = paths.as_slice() else {
        return Err("expected one matching source".into());
    };
    if !Path::new(path)
        .components()
        .all(|p| matches!(p, std::path::Component::Normal(_)))
        || fs::read(member.join(path))? != source
    {
        return Err("native source differs".into());
    }
    Ok(
        json!({"workspaceLockSha256":sha(&lock),"nativePackageLockSha256":member_hash,"sourcePath":path}),
    )
}

fn prepare(root: &Path, requests: &Path) -> Result<()> {
    for name in ["Cargo.toml", "Cargo.lock", "observer.rs"] {
        fs::copy(requests.join(name), root.join(name))?;
    }
    Ok(())
}
fn write_foundation(root: &Path, original: &Path) -> Result<()> {
    let foundation = String::from_utf8(git_blob(original, "foundation.rs")?)?;
    let (_, body) = foundation
        .split_once("pub(crate) fn valid_hex(")
        .ok_or("missing valid_hex")?;
    let (body, _) = body.split_once("\n}").ok_or("incomplete valid_hex")?;
    fs::write(
        root.join("foundation.rs"),
        format!("pub(crate) fn valid_hex({body}\n}}"),
    )?;
    Ok(())
}
fn observe(root: &Path, flags: &str, input: &Path) -> Result<Vec<u8>> {
    let result = Command::new("cargo")
        .args(["+1.97.1", "run", "--locked", "--offline", "--quiet"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("RUSTFLAGS", flags)
        .stdin(fs::File::open(input)?)
        .stderr(Stdio::inherit())
        .output()?;
    if !result.status.success() {
        return Err("response observer failed".into());
    }
    Ok(result.stdout)
}
fn compare_codecs(
    root: &Path,
    whole: &Path,
    original: &Path,
    source: &[u8],
    input: &Path,
    candidate_adapter: &Path,
    reference_adapter: &Path,
) -> Result<Vec<u8>> {
    let mut observations = Vec::new();
    for (reference, adapter) in [(false, candidate_adapter), (true, reference_adapter)] {
        let dir = root.join(if reference { "reference" } else { "candidate" });
        fs::create_dir(&dir)?;
        prepare(&dir, &whole.join("command-requests"))?;
        fs::copy(adapter, dir.join("peer.rs"))?;
        let mut flags = String::from("-Adead_code --check-cfg=cfg(reference)");
        if reference {
            fs::write(dir.join("codec.rs"), git_blob(original, "command_wire.rs")?)?;
            write_foundation(&dir, original)?;
            flags.push_str(" --cfg reference");
        } else {
            fs::write(dir.join("codec.rs"), source)?;
        }
        observations.push(observe(&dir, &flags, input)?);
    }
    if observations[0] != observations[1] {
        return Err("reference and projected codec observations differ".into());
    }
    Ok(observations.remove(0))
}
fn main() -> Result<()> {
    let args = Args::parse();
    let whole = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let responses = whole.join("command-responses");
    let requests = whole.join("command-requests");
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    prepare(root, &requests)?;
    let mut flags = String::from("-Adead_code --check-cfg=cfg(reference)");
    let generated = matches!(args.mode, Mode::Generated { .. });
    let mut receipt = json!({"referenceRevision":ORIGINAL});
    match args.mode {
        Mode::Requests {
            compiler,
            original,
            package,
            workspace_lock,
        } => {
            let result = requests::compare(
                root,
                whole,
                &compiler,
                &original,
                package.as_deref(),
                workspace_lock.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        Mode::Shapes { compiler, original } => {
            let result = shapes::compare(root, whole, &compiler, &original)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        Mode::Reference { original } => {
            let source = git_blob(&original, "command_wire.rs")?;
            receipt["referenceSourceSha256"] = json!(sha(&source));
            fs::write(root.join("codec.rs"), source)?;
            write_foundation(root, &original)?;
            fs::copy(
                responses.join("reference_envelope.rs"),
                root.join("peer.rs"),
            )?;
            flags.push_str(" --cfg reference");
        }
        Mode::Generated {
            compiler,
            workspace,
            workspace_lock,
        } => {
            let portable = whole.join("../../oddlang/portable");
            let projection = root.join("projection");
            let status = Command::new(&compiler)
                .args(["build", "wire-protocol", "rust"])
                .args(
                    [
                        "command-response-shapes-v1/graph.json",
                        "command-response-shapes-v1/rust.json",
                        "command-responses-v1/graph.json",
                        "command-responses-v1/rust.json",
                    ]
                    .map(|p| portable.join(p)),
                )
                .arg(&projection)
                .status()?;
            if !status.success() {
                return Err("response projection failed".into());
            }
            let source = fs::read(projection.join("wire.rs"))?;
            if let (Some(dir), Some(pin)) = (workspace, workspace_lock) {
                receipt["workspace"] =
                    workspace_evidence(&dir, &pin, "command_responses", &source)?;
            }
            fs::write(root.join("codec.rs"), source)?;
            fs::copy(requests.join("generated.rs"), root.join("peer.rs"))?;
            receipt["projectionLockSha256"] =
                json!(sha(&fs::read(projection.join("projection.lock"))?));
            receipt["compilerSha256"] = json!(sha(&fs::read(compiler)?));
        }
    }
    let rows: Vec<(String, String, bool)> =
        serde_json::from_slice(include_bytes!("envelope-cases.json"))?;
    let mut inputs = fs::File::create(root.join("input"))?;
    let mut expected = Vec::new();
    let mut names = BTreeSet::new();
    let mut inventory = BTreeMap::new();
    let mut samples = BTreeMap::new();
    let mut accepted_count = 0;
    for (name, raw, accepted) in &rows {
        if !names.insert(name) {
            return Err(format!("duplicate case {name}").into());
        }
        let packet: Value = serde_json::from_str(raw)?;
        let op = packet["op"].as_str().ok_or("missing operation")?;
        let wire = hex(raw.as_bytes());
        if *accepted {
            accepted_count += 1;
            if name.starts_with(&format!("{op}/")) {
                samples.entry(op.to_owned()).or_insert_with(|| wire.clone());
            }
        }
        writeln!(
            inputs,
            "{}",
            json!([if generated { op } else { "response" }, wire])
        )?;
        expected.push(accepted.then_some(wire));
        inventory.insert(
            name,
            json!({"accepted":accepted,"wireBytes":raw.len(),"wireSha256":sha(raw.as_bytes())}),
        );
    }
    if generated {
        let reference: Value = serde_json::from_slice(REFERENCE)?;
        if reference["referenceRevision"] != ORIGINAL || reference["cases"] != json!(inventory) {
            return Err("corpus differs from pinned mainline receipt".into());
        }
        for op in OPERATIONS {
            let sample = samples.get(op).ok_or("missing accepted operation")?;
            for other in OPERATIONS.into_iter().filter(|other| *other != op) {
                writeln!(inputs, "{}", json!([other, sample]))?;
                expected.push(None);
            }
        }
    }
    drop(inputs);
    let bytes = observe(root, &flags, &root.join("input"))?;
    let output = std::str::from_utf8(&bytes)?;
    let lines: Vec<_> = output.split_inclusive('\n').collect();
    if lines.len() != expected.len() {
        return Err("incorrect number of responses".into());
    }
    for (index, (line, want)) in lines.iter().zip(expected.iter()).enumerate() {
        if serde_json::from_str::<Option<String>>(line)? != *want {
            return Err(format!(
                "unexpected response for {}",
                rows.get(index)
                    .map(|r| r.0.as_str())
                    .unwrap_or("cross-decoder case")
            )
            .into());
        }
    }
    if generated {
        receipt["cases"] = json!(rows.len());
        receipt["accepted"] = json!(accepted_count);
        receipt["wrongDecoderRejections"] = json!(lines.len() - rows.len());
        receipt["observationsSha256"] = json!(sha(lines[..rows.len()].concat().as_bytes()));
        receipt["referenceReceiptSha256"] = json!(sha(REFERENCE));
    } else {
        receipt["cases"] = json!(inventory);
    }
    receipt.sort_all_objects();
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
