use super::*;
use serde_json::json;
use std::io::Write;
use tempfile::tempdir;

fn append(path: &Path, text: &str) -> Result<()> {
    writeln!(fs::OpenOptions::new().append(true).open(path)?, "\n{text}")?;
    Ok(())
}

fn build(work: &Path, cache: &Path, host: &str, bins: &[&str], isolated: &Path) -> Result<()> {
    let mut build = Command::new("cargo");
    build
        .current_dir(work)
        .env("MARMISH_BUILD_COMMIT", PIN)
        .args([
            &format!("+{TOOLCHAIN}"),
            "build",
            "--locked",
            "--offline",
            "--release",
            "--target",
            host,
            "--target-dir",
        ])
        .arg(cache)
        .args([
            "-p",
            "marmish-native",
            "--message-format=json-render-diagnostics",
        ]);
    for bin in bins {
        build.args(["--bin", bin]);
    }
    let messages = output(&mut build)?;
    let mut pending: std::collections::BTreeSet<_> = bins.iter().copied().collect();
    pending.insert("marmish_native");
    let root = work.canonicalize()?;
    for line in messages.lines() {
        let record: serde_json::Value = serde_json::from_str(line)?;
        if record["reason"] != "compiler-artifact" {
            continue;
        }
        let Some(name) = record["target"]["name"].as_str() else {
            continue;
        };
        if !pending.contains(name) {
            continue;
        }
        let source = Path::new(
            record["target"]["src_path"]
                .as_str()
                .ok_or("missing source path")?,
        )
        .canonicalize()?;
        if !source.starts_with(&root) || record["fresh"] != false {
            return Err(format!(
                "expected a fresh compilation of {name} from the disposable source tree"
            )
            .into());
        }
        if bins.contains(&name) {
            let executable = record["executable"].as_str().ok_or("missing executable")?;
            fs::copy(executable, isolated.join(name))?;
        }
        pending.remove(name);
    }
    if !pending.is_empty() {
        return Err(format!("missing compilation records: {pending:?}").into());
    }
    Ok(())
}

fn upstream(repo: &Path, cache: &Path, host: &str, isolated: &Path) -> Result<()> {
    let work = tempdir()?;
    let archive = work.path().join("sources.tar");
    run(Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["archive", "--output"])
        .arg(&archive)
        .arg(PIN))?;
    run(Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(work.path()))?;
    let src = work.path().join("crates/marmish-native/src");
    append(
        &src.join("command_config.rs"),
        include_str!("../upstream_api.rs"),
    )?;
    append(
        &src.join("lib.rs"),
        "pub use command_config::policy_observation;",
    )?;
    fs::write(
        src.join("bin/policy-observer.rs"),
        concat!(
            "use marmish_native::policy_observation;\n",
            include_str!("../driver.rs")
        ),
    )?;
    build(work.path(), cache, host, &["policy-observer"], isolated)
}

fn owner(src: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    for dir in fs::read_dir(src)? {
        let dir = dir?;
        if dir.file_name().to_string_lossy().starts_with("group_") && dir.file_type()?.is_dir() {
            for file in fs::read_dir(dir.path())? {
                let file = file?.path();
                if file.extension().is_some_and(|s| s == "rs")
                    && fs::read_to_string(&file)?.contains("fn room_config(")
                {
                    found.push(file);
                }
            }
        }
    }
    if found.len() != 1 {
        return Err(format!("expected one room_config owner, found {}", found.len()).into());
    }
    Ok(found.remove(0))
}

fn copy_sources(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let dest = dest.join(name);
        if entry.file_type()?.is_dir() {
            fs::create_dir(&dest)?;
            copy_sources(&entry.path(), &dest)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), dest)?;
        } else {
            return Err(format!("unexpected file type: {:?}", entry.path()).into());
        }
    }
    Ok(())
}

fn generated(workspace: &Path, cache: &Path, host: &str, isolated: &Path) -> Result<()> {
    let scratch = tempdir()?;
    copy_sources(workspace, scratch.path())?;
    let src = scratch.path().join("marmish-native/src");
    let path = owner(&src)?;
    let module = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("invalid module name")?;
    let exports = "{policy_observation, observe_room_config}";
    append(&path, include_str!("../generated_api.rs"))?;
    append(
        &path.with_file_name("mod.rs"),
        &format!("pub use {module}::{exports};"),
    )?;
    append(
        &src.join("lib.rs"),
        &format!("pub use foreground::{exports};"),
    )?;
    fs::write(
        src.join("bin/generated-observer.rs"),
        concat!(
            "use marmish_native::policy_observation;\n",
            include_str!("../driver.rs")
        ),
    )?;
    fs::write(
        src.join("bin/room-observer.rs"),
        include_str!("../room_observer.rs"),
    )?;
    build(
        scratch.path(),
        cache,
        host,
        &["generated-observer", "room-observer"],
        isolated,
    )
}

pub fn qualify(original: &Path, cache: &Path, workspace: &Path, receipt: &Path) -> Result<()> {
    let version = output(Command::new("rustc").args([&format!("+{TOOLCHAIN}"), "-vV"]))?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    let isolated = tempdir()?;
    let bins = isolated.path();
    upstream(original, cache, host, bins)?;
    generated(workspace, cache, host, bins)?;
    let mut observers = serde_json::Map::new();
    for name in ["policy-observer", "generated-observer", "room-observer"] {
        observers.insert(name.into(), json!(sha(&fs::read(bins.join(name))?)));
    }
    let decisions = replay::policy(
        &bins.join("policy-observer"),
        &bins.join("generated-observer"),
    )?;
    let file_cases = replay::room(&bins.join("room-observer"))?;
    let receipt_data = json!({
        "rebuiltObserverSha256":observers,"upstreamCommit":PIN, "rustToolchain":TOOLCHAIN, "host":host,
        "workspaceLockSha256":sha(&fs::read(workspace.join("workspace.lock"))?),
        "foregroundSourceSha256":sha(&fs::read(owner(&workspace.join("marmish-native/src"))?)?),
        "policyDecisions":decisions,"roomConfigFileCases":file_cases
    });
    write_json(receipt, &receipt_data)?;
    println!("{receipt_data:#}");
    Ok(())
}
