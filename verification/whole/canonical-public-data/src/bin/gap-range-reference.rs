//! Check the JSON data contract of the native gap reason and range types.
use clap::{Parser, ValueEnum};
use public_data_qualification::{Result, capture, load, portable, require, sha, text, whole};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
const UPSTREAM: &str = "af54339d250290490d62d4475a77539b7e739071";
const FRAMEWORK: &str = "3c01918dd430afcf3b57a21a5b24191f99333025";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
const COPY: &str = "fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }";
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Observe,
    Project,
}
#[derive(Parser)]
struct Args {
    #[arg(value_enum)]
    mode: Mode,
    checkout: PathBuf,
    target: PathBuf,
    output: PathBuf,
}
fn command(tool: &str, dir: &Path) -> Command {
    let mut c = Command::new(tool);
    c.current_dir(dir).env_clear();
    for key in [
        "HOME",
        "PATH",
        "TMPDIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "SDKROOT",
    ] {
        if let Some(v) = env::var_os(key) {
            c.env(key, v);
        }
    }
    c
}
fn checked(c: &mut Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
    let out = capture(c, 1800, input)?;
    require(
        out.status.success(),
        format!(
            "{c:?}: {} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ),
    )?;
    Ok(out.stdout)
}
fn pinned(dir: &Path, expected: &str) -> Result {
    let head = checked(command("git", dir).args(["rev-parse", "HEAD"]), None)?;
    let status = checked(
        command("git", dir).args(["status", "--porcelain", "--untracked-files=all"]),
        None,
    )?;
    require(
        std::str::from_utf8(&head)?.trim() == expected && status.is_empty(),
        format!("expected clean pinned checkout at {expected}"),
    )
}
fn host(dir: &Path) -> Result<String> {
    let b = checked(command("rustc", dir).args(["+1.97.1", "-vV"]), None)?;
    let s = std::str::from_utf8(&b)?;
    require(
        s.lines().any(|s| s == format!("commit-hash: {COMPILER}")),
        "unexpected Rust compiler",
    )?;
    let rows: Vec<_> = s.lines().filter_map(|s| s.strip_prefix("host: ")).collect();
    if let [one] = rows.as_slice() {
        Ok(one.to_string())
    } else {
        Err("expected one host".into())
    }
}
fn rows(bytes: &[u8]) -> Result<Vec<Value>> {
    Ok(std::str::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?)
}
fn array(v: &Value) -> Result<&Vec<Value>> {
    Ok(v.as_array().ok_or("expected array")?)
}
fn errors(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .filter(|v| v["level"] == "error")
        .filter_map(|v| v["code"]["code"].as_str().map(str::to_owned))
        .collect()
}
fn fixture() -> PathBuf {
    whole().join("gap-range-reference")
}
fn source_pin() -> Result<String> {
    sha(&whole().join("canonical-public-data/src/bin/gap-range-reference.rs"))
}
fn observe(original: &Path, target: &Path, host: &str) -> Result<Value> {
    let lane = fixture();
    let build = checked(
        command("cargo", original)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--lib",
                "-p",
                "marmish-engine",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(target)
            .arg("--message-format=json"),
        None,
    )?;
    let artifacts = rows(&build)?;
    let mut libs = BTreeMap::new();
    let mut links = BTreeSet::new();
    for r in &artifacts {
        if r["reason"] == "build-script-executed" {
            for l in array(&r["linked_paths"])? {
                links.insert(text(l)?.to_owned());
            }
        }
        if r["reason"] != "compiler-artifact" || r["target"]["kind"] != json!(["lib"]) {
            continue;
        }
        let name = text(&r["target"]["name"])?;
        if !["marmish_engine", "serde_json"].contains(&name) {
            continue;
        }
        let paths: Vec<_> = array(&r["filenames"])?
            .iter()
            .map(text)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(Path::new)
            .filter(|p| p.extension().is_some_and(|e| e == "rlib"))
            .collect();
        if !paths.iter().any(|p| p.starts_with(target.join(host))) {
            continue;
        }
        let [lib] = paths.as_slice() else {
            return Err("ambiguous library artifact".into());
        };
        if name == "marmish_engine" {
            require(
                Path::new(text(&r["manifest_path"])?).canonicalize()?
                    == original.join("crates/marmish-engine/Cargo.toml"),
                "unexpected engine package",
            )?;
        }
        require(
            libs.insert(name, lib.to_owned()).is_none(),
            "duplicate library artifact",
        )?;
    }
    require(libs.len() == 2, "missing selected libraries")?;
    let seal = || -> Result<BTreeMap<&str, String>> {
        libs.iter()
            .map(|(name, path)| Ok((*name, sha(path)?)))
            .collect()
    };
    let hashes = seal()?;
    let scratch = tempfile::tempdir()?;
    let work = scratch.path();
    let rustc = || {
        let mut c = command("rustc", work);
        c.args(["+1.97.1", "--edition=2024", "--target", host]);
        for (name, lib) in &libs {
            c.arg("--extern").arg(format!("{name}={}", lib.display()));
        }
        for path in [
            target.join(host).join("release/deps"),
            target.join("release/deps"),
        ] {
            c.arg("-L").arg(format!("dependency={}", path.display()));
        }
        for path in &links {
            c.arg("-L").arg(path);
        }
        c
    };
    let exe = work.join("probe");
    checked(
        rustc()
            .args(["--cfg", "with_range"])
            .arg(lane.join("probe.rs"))
            .arg("-o")
            .arg(&exe),
        None,
    )?;
    let control = work.join("copy_control.rs");
    fs::write(&control, COPY)?;
    let result = capture(
        rustc()
            .args(["--error-format=json", "--emit=metadata"])
            .arg(control)
            .arg("-o")
            .arg(work.join("control.rmeta")),
        120,
        None,
    )?;
    let codes = errors(&rows(&result.stderr)?);
    require(
        !result.status.success() && codes == ["E0277"],
        "unexpected Copy control",
    )?;
    let vectors = fs::read(lane.join("cases.json"))?;
    let cases: Value = serde_json::from_slice(&vectors)?;
    let observed: Value = serde_json::from_slice(&checked(
        &mut command(exe.to_str().ok_or("UTF-8 probe path")?, work),
        Some(&vectors),
    )?)?;
    let ids = |v: &Value| -> Result<Vec<String>> {
        array(v)?
            .iter()
            .map(|r| Ok(text(&r["id"])?.to_owned()))
            .collect()
    };
    let expected = ids(&cases)?;
    require(
        expected.iter().collect::<BTreeSet<_>>().len() == expected.len()
            && ids(&observed)? == expected,
        "incomplete or ambiguous case results",
    )?;
    pinned(original, UPSTREAM)?;
    require(
        hashes == seal()?,
        "selected release library changed during probe",
    )?;
    Ok(
        json!({"original":UPSTREAM,"host":host,"compiler":COMPILER,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,"modelSha256":sha(&original.join("crates/marmish-engine/src/model.rs"))?,"casesSha256":sha(&lane.join("cases.json"))?,"probeSha256":sha(&lane.join("probe.rs"))?,"observerSha256":source_pin()?,"copyControl":codes,"librarySha256":hashes,"results":observed}),
    )
}
fn project(framework: &Path, target: &Path, host: &str) -> Result<Value> {
    let lane = fixture();
    let data = portable();
    let spec_file = data.join("engine-gap-reason-package.json");
    let reference = data.join("engine-gap-reason-v1/package.lock");
    let archive = lane.join(format!("original-{host}.json"));
    let original = load(&archive)?;
    require(
        original["casesSha256"] == sha(&lane.join("cases.json"))?
            && original["probeSha256"] == sha(&lane.join("probe.rs"))?,
        "reference wire probe changed",
    )?;
    let all = load(&lane.join("cases.json"))?;
    let cases: Vec<_> = array(&all)?
        .iter()
        .filter(|v| v["type"] == "reason")
        .collect();
    let ids = cases
        .iter()
        .map(|v| text(&v["id"]))
        .collect::<Result<BTreeSet<_>>>()?;
    let expected: Vec<_> = array(&original["results"])?
        .iter()
        .filter(|v| v["id"].as_str().is_some_and(|id| ids.contains(id)))
        .collect();
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    checked(
        command("cargo", framework)
            .args([
                "+1.96.1",
                "build",
                "--locked",
                "--offline",
                "--bin",
                "oddlang",
                "--target-dir",
            ])
            .arg(target),
        None,
    )?;
    let compiler = work.join("oddlang");
    fs::copy(target.join("debug/oddlang"), &compiler)?;
    let compiler_sha = sha(&compiler)?;
    let spec = load(&spec_file)?;
    let staged = work.join("inputs");
    fs::create_dir(&staged)?;
    let mut pins = vec![&spec["cargoLock"]];
    for component in array(&spec["components"])? {
        pins.extend([&component["graph"], &component["overlay"]]);
    }
    for pin in pins {
        let rel = Path::new(text(&pin["path"])?);
        require(
            rel.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "unsafe input path",
        )?;
        let src = data.join(rel);
        let dest = staged.join(rel);
        require(
            sha(&src)? == pin["sha256"],
            "component graph, overlay or Cargo lock changed",
        )?;
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(src, dest)?;
    }
    let manifest = staged.join(spec_file.file_name().unwrap());
    fs::copy(&spec_file, &manifest)?;
    let product = work.join("product");
    checked(
        command(compiler.to_str().ok_or("UTF-8 compiler path")?, work)
            .args(["build", "native-package", "rust"])
            .arg(manifest)
            .arg(&product),
        None,
    )?;
    require(
        fs::read(product.join("package.lock"))? == fs::read(&reference)?,
        "generated package lock changed",
    )?;
    for (path, hash) in load(&reference)?["generatedFiles"]
        .as_object()
        .ok_or("generated file map")?
    {
        require(
            sha(&product.join(path))? == *hash,
            "generated files differ from seal",
        )?;
    }
    let caller = work.join("caller");
    fs::create_dir_all(caller.join("src"))?;
    for (src, dst) in [
        ("caller.toml", "Cargo.toml"),
        ("caller.lock", "Cargo.lock"),
        ("probe.rs", "src/main.rs"),
    ] {
        fs::copy(lane.join(src), caller.join(dst))?;
    }
    let cargo = |verb: &str| {
        let mut cmd = command("cargo", &caller);
        cmd.args([
            "+1.97.1",
            verb,
            "--offline",
            "--locked",
            "--release",
            "--target",
            host,
        ]);
        cmd
    };
    let observations: Value = serde_json::from_slice(&checked(
        cargo("run").arg("--quiet"),
        Some(&serde_json::to_vec(&cases)?),
    )?)?;
    require(
        observations == json!(expected) && expected.len() == cases.len(),
        "candidate wire outcomes differ",
    )?;
    fs::write(caller.join("src/main.rs"), COPY)?;
    let result = capture(cargo("check").arg("--message-format=json"), 120, None)?;
    let messages: Vec<_> = rows(&result.stdout)?
        .into_iter()
        .filter(|r| r["reason"] == "compiler-message")
        .map(|r| r["message"].clone())
        .collect();
    let codes = errors(&messages);
    require(
        !result.status.success() && codes == ["E0277"] && original["copyControl"] == json!(codes),
        "unexpected Copy diagnostic",
    )?;
    pinned(framework, FRAMEWORK)?;
    Ok(
        json!({"host":host,"rustcCommit":COMPILER,"framework":FRAMEWORK,"frameworkBinarySha256":compiler_sha,"callerLockSha256":sha(&lane.join("caller.lock"))?,"callerManifestSha256":sha(&lane.join("caller.toml"))?,"manifestSha256":sha(&spec_file)?,"packageLockSha256":sha(&reference)?,"referenceSha256":sha(&archive)?,"verifierSha256":source_pin()?,"copyControl":codes,"observations":observations}),
    )
}
fn main() -> Result {
    let args = Args::parse();
    let checkout = args.checkout.canonicalize()?;
    let target = std::path::absolute(&args.target)?;
    pinned(
        &checkout,
        match args.mode {
            Mode::Observe => UPSTREAM,
            Mode::Project => FRAMEWORK,
        },
    )?;
    require(
        !target.starts_with(&checkout),
        "use an external Cargo target directory",
    )?;
    fs::create_dir_all(&target)?;
    let target = target.canonicalize()?;
    require(
        !target.starts_with(&checkout),
        "use an external Cargo target directory",
    )?;
    let host = host(&checkout)?;
    let mut doc = match args.mode {
        Mode::Observe => observe(&checkout, &target, &host)?,
        Mode::Project => project(&checkout, &target, &host)?,
    };
    doc.sort_all_objects();
    fs::write(args.output, public_data_qualification::json_bytes(&doc)?)?;
    Ok(())
}
