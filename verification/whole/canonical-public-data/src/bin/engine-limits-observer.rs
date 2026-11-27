//! Compare the pinned engine and generated workspace in isolated child processes.
use clap::Parser;
use public_data_qualification::{
    Result, assembly::ORIGINAL, capture, json_bytes, require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    original_binaries: PathBuf,
    generated: PathBuf,
    reference_target: PathBuf,
}

#[derive(Default)]
struct Libraries {
    files: BTreeMap<String, PathBuf>,
    native: BTreeSet<String>,
}

const LIBS: [&str; 2] = ["libc", "marmish_engine"];

fn engine_libraries(workspace: &Path, target: &Path) -> Result<Libraries> {
    let records = stdout(
        Command::new("cargo")
            .current_dir(workspace)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--package",
                "marmish-engine",
                "--lib",
                "--message-format",
                "json",
                "--target-dir",
            ])
            .arg(target),
    )?;
    let mut libs = Libraries::default();
    for line in records.lines() {
        let v: Value = serde_json::from_str(line)?;
        match v["reason"].as_str() {
            Some("compiler-artifact") => {
                let name = v["target"]["name"].as_str().ok_or("missing target name")?;
                if LIBS.contains(&name) {
                    for path in v["filenames"].as_array().ok_or("missing artifact files")? {
                        let path = PathBuf::from(path.as_str().ok_or("invalid filename")?);
                        if path.extension().is_some_and(|ext| ext == "rlib") {
                            require(
                                libs.files.insert(name.into(), path).is_none(),
                                format!("ambiguous {name}"),
                            )?;
                        }
                    }
                }
            }
            Some("build-script-executed") => {
                for path in v["linked_paths"].as_array().ok_or("missing linked paths")? {
                    libs.native
                        .insert(path.as_str().ok_or("invalid native path")?.into());
                }
            }
            _ => {}
        }
    }
    Ok(libs)
}

fn compile(work: &Path, role: &str, libs: Libraries) -> Result<(PathBuf, Value)> {
    require(
        libs.files.len() == LIBS.len(),
        format!("incomplete {role} libraries"),
    )?;
    let exe = work.join(role);
    let deps = libs.files["libc"]
        .parent()
        .ok_or("missing dependency directory")?;
    let mut cmd = Command::new("rustc");
    cmd.args(["+1.97.1", "--edition=2021"])
        .arg(whole().join("engine-limits/observe.rs"))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()));
    let mut hashes = json!({});
    for (name, path) in libs.files {
        hashes[&name] = json!(sha(&path)?);
        cmd.arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    for path in libs.native {
        cmd.arg("-L").arg(path);
    }
    run(cmd.arg("-o").arg(&exe))?;
    Ok((exe, hashes))
}

fn limited(cmd: &mut Command, work: &Path, soft: libc::rlim_t) -> Result<Output> {
    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    limits.rlim_cur = soft;
    // Only the child changes its inherited soft limit. setrlimit needs no allocation.
    unsafe {
        cmd.current_dir(work).pre_exec(move || {
            if libc::setrlimit(libc::RLIMIT_NOFILE, &limits) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    capture(cmd, 360, None)
}

fn library_cases(exe: &Path, work: &Path) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for soft in [1024, 65536, 65537] {
        let result = limited(&mut Command::new(exe), work, soft)?;
        let values: Vec<u64> = std::str::from_utf8(&result.stdout)?
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        let expected = if cfg!(target_os = "macos") {
            soft.min(65536)
        } else {
            soft
        };
        require(
            result.status.success()
                && result.stderr.is_empty()
                && values.len() == 3
                && values[..2] == [soft, expected],
            format!("unexpected descriptor limits: {result:?}"),
        )?;
        rows.push(json!({"beforeSoft":soft,"afterSoft":expected,"hard":values[2],"twoCallsIdempotent":true,"bothDescriptorFlagsPreserved":true}));
    }
    Ok(rows)
}

fn startup(work: &Path, args: &Args) -> Result<Value> {
    let mut receipt = json!({"originalBinaries":{}, "generatedBinaries":{}, "argvCases":[]});
    let mut rows = Vec::new();
    let image = work.join("deny-rlimit.dylib");
    if cfg!(target_os = "macos") {
        run(Command::new("xcrun")
            .args([
                "--sdk",
                "macosx",
                "clang",
                "-Wall",
                "-Werror",
                "-dynamiclib",
            ])
            .arg(whole().join("engine-limits/deny_darwin.c"))
            .arg("-o")
            .arg(&image))?;
    }
    for name in ["marmish", "marmishd"] {
        let old = args.original_binaries.join(name);
        let new = args.generated.join("target/release").join(name);
        receipt["originalBinaries"][name] = json!(sha(&old)?);
        receipt["generatedBinaries"][name] = json!(sha(&new)?);
        for (argument, fault) in [
            ("--help", false),
            ("--version", false),
            ("--an-unknown-argument", false),
            ("release-info", false),
            ("--an-unknown-argument", true),
        ] {
            if fault && !cfg!(target_os = "macos") {
                continue;
            }
            let execute = |exe: &Path| -> Result<Output> {
                let mut cmd = Command::new(exe);
                if fault {
                    cmd.env("DYLD_INSERT_LIBRARIES", &image);
                }
                limited(cmd.arg(argument), work, 65537)
            };
            let a = execute(&old)?;
            let b = execute(&new)?;
            require(
                a.status == b.status && a.stdout == b.stdout && a.stderr == b.stderr,
                format!("{name} {argument} startup differs: {a:?}, {b:?}"),
            )?;
            if fault {
                require(
                    a.status.code() == Some(1)
                        && a.stdout.is_empty()
                        && a.stderr == b"marmish: process_limits\n",
                    "resource fault did not preempt argument parsing",
                )?;
            }
            let exit = a
                .status
                .code()
                .or_else(|| a.status.signal().map(|s| -s))
                .ok_or("unknown wait status")?;
            rows.push(json!({"binary":name,"arguments":[argument],"exit":exit,"stdout":String::from_utf8(a.stdout)?,
                "stderr":String::from_utf8(a.stderr)?,"fault":fault.then_some("getrlimit NOFILE returns EPERM")}));
        }
    }
    receipt["argvCases"] = json!(rows);
    Ok(receipt)
}

fn main() -> Result {
    let mut args = Args::parse();
    args.original = args.original.canonicalize()?;
    args.generated = args.generated.canonicalize()?;
    args.original_binaries = args.original_binaries.canonicalize()?;
    args.reference_target = std::path::absolute(args.reference_target)?;
    require(
        stdout(
            Command::new("git")
                .arg("-C")
                .arg(&args.original)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == ORIGINAL,
        "unexpected original revision",
    )?;
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    let (a, old_hashes) = compile(
        work,
        "original",
        engine_libraries(&args.original, &args.reference_target)?,
    )?;
    let (b, new_hashes) = compile(
        work,
        "generated",
        engine_libraries(&args.generated, &args.generated.join("target"))?,
    )?;
    let cases = library_cases(&a, work)?;
    require(
        cases == library_cases(&b, work)?,
        "process resource observation differs",
    )?;
    let mut receipt = startup(work, &args)?;
    receipt["libraryCases"] = json!(cases);
    receipt["originalLibraries"] = old_hashes;
    receipt["generatedLibraries"] = new_hashes;
    receipt["observerSha256"] = json!(sha(&whole().join("engine-limits/observe.rs"))?);
    receipt["darwinFaultObserverSha256"] =
        json!(sha(&whole().join("engine-limits/deny_darwin.c"))?);
    io::stdout().lock().write_all(&json_bytes(&receipt)?)?;
    Ok(())
}
