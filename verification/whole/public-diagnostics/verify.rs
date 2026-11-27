use public_data_qualification::{
    Result,
    assembly::{DARWIN, LINUX, ORIGINAL, checkout},
    capture, capture_stderr, require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{fs, io, path::Path, process::Command};

fn probe() -> std::path::PathBuf {
    whole().join("public-diagnostics/probe.rs")
}

pub fn observe(
    library: &Path,
    host: &str,
    version: &str,
    work: &Path,
    host_deps: Option<&Path>,
) -> Result<Value> {
    let platform = match host {
        DARWIN => "macos",
        LINUX => "linux",
        _ => return Err(format!("unsupported host: {host}").into()),
    };
    let caller = work.join("observer");
    let mut compiler = Command::new("rustc");
    compiler
        .args(["+1.97.1", "--edition=2021", "--target", host])
        .arg(probe())
        .arg("--extern")
        .arg(format!("marmish_native={}", library.display()));
    let deps = library
        .parent()
        .ok_or("missing library directory")?
        .join("deps");
    for dir in Some(deps.as_path()).into_iter().chain(host_deps) {
        compiler
            .arg("-L")
            .arg(format!("dependency={}", dir.display()));
    }
    run(compiler.arg("-o").arg(&caller))?;

    // Field order and literal UTF-8 are observable in the newline-delimited event stream.
    let context = json!({"architecture":"aarch64","event":"context","platform":platform,"schema":1,"version":version});
    let event = |stage: &str, category: &str| json!({"category":category,"event":"diagnostic","schema":1,"stage":stage});
    let cases = [
        ("disabled", "before", vec![]),
        (
            "sequence",
            "after",
            vec![
                context.clone(),
                event("quoted \"\\", "unicode 雪\n\t\u{1}"),
                event("after", "begin"),
                context.clone(),
                event("", ""),
            ],
        ),
        (
            "cross-thread",
            "initial",
            vec![context, event("joined", "visible")],
        ),
        ("closed-sink", "after-error", vec![]),
    ];
    let mut rows = json!({});
    for (case, state, events) in cases {
        let sink = if case == "closed-sink" {
            let (read, write) = io::pipe()?;
            drop(read);
            Some(write.into())
        } else {
            None
        };
        let observed = capture_stderr(Command::new(&caller).arg(case), 10, None, sink)?;
        let expected: String = events.iter().map(|event| format!("{event}\n")).collect();
        require(
            observed.status.success()
                && observed.stdout == format!("{state}\n").as_bytes()
                && observed.stderr == expected.as_bytes(),
            format!("unexpected {case} output: {observed:?}"),
        )?;
        rows[case] = json!({"exitCode":observed.status.code(),"stdout":String::from_utf8(observed.stdout)?,
            "stderr":String::from_utf8(observed.stderr)?,"sink":if case=="closed-sink" {"closed pipe"} else {"captured"}});
    }
    Ok(rows)
}

pub fn baseline(original: &Path, target: &Path, receipt: &Path) -> Result {
    let original = original.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    let compiler = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = compiler
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("missing host")?;
    require([DARWIN, LINUX].contains(&host), "unsupported host")?;
    fs::create_dir_all(target)?;
    let target = target.canonicalize()?;
    let build = capture(
        Command::new("cargo")
            .current_dir(&original)
            .args([
                "+1.97.1",
                "build",
                "--locked",
                "--offline",
                "--release",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(&target)
            .args(["-p", "marmish-native", "--lib", "--message-format=json"]),
        1500,
        None,
    )?;
    require(
        build.status.success(),
        String::from_utf8_lossy(&build.stderr),
    )?;
    let manifest = original.join("crates/marmish-native/Cargo.toml");
    let mut matches = Vec::new();
    for line in std::str::from_utf8(&build.stdout)?.lines() {
        let row: Value = serde_json::from_str(line)?;
        if row["reason"] == "compiler-artifact"
            && row["target"]["kind"] == json!(["lib"])
            && Path::new(
                row["manifest_path"]
                    .as_str()
                    .ok_or("missing manifest path")?,
            )
            .canonicalize()?
                == manifest
        {
            matches.push(row);
        }
    }
    let [row] = matches.as_slice() else {
        return Err("expected exactly one native library build".into());
    };
    let libs: Vec<_> = row["filenames"]
        .as_array()
        .ok_or("missing filenames")?
        .iter()
        .filter_map(Value::as_str)
        .map(Path::new)
        .filter(|p| p.extension().is_some_and(|e| e == "rlib"))
        .collect();
    let [lib] = libs.as_slice() else {
        return Err("expected one rlib".into());
    };
    require(
        lib.canonicalize()? == target.join(host).join("release/libmarmish_native.rlib"),
        "unexpected compiled artifact",
    )?;
    let work = tempfile::tempdir()?;
    let rows = observe(
        lib,
        host,
        "0.3.1",
        work.path(),
        Some(&target.join("release/deps")),
    )?;
    let result = json!({"original":ORIGINAL,"compiler":compiler.lines().collect::<Vec<_>>(),"host":host,
        "sourceSha256":sha(&original.join("crates/marmish-native/src/diagnostics.rs"))?,
        "packageManifestSha256":sha(&manifest)?,"cargoLockSha256":sha(&original.join("Cargo.lock"))?,
        "probeSha256":sha(&probe())?,"librarySha256":sha(lib)?,"observations":rows});
    fs::write(
        receipt,
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    Ok(())
}
