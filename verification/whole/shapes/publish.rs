//! Recover or reuse profile cells, compare independent observations and seal a new archive.
use super::*;
use public_data_qualification::stdout;
use rustix::fs::{CWD, RenameFlags, renameat_with};
use std::{
    env,
    io::Write,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn write(path: &Path, mut value: Value) -> Result<()> {
    value.sort_all_objects();
    let mut file = fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut file, &value)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn artifact(original: &Path, pins: &mut Pins, dir: &Path, cell: &Value) -> Result<Value> {
    let graph = dir.join("shapes.json");
    let lock = dir.join("shapes.lock");
    let digest = sha(&graph)?;
    let semantic = pins.validate(original, cell, &digest, &load(&lock)?)?;
    Ok(
        json!({"status":"candidate","graphSha256":digest,"lockSha256":sha(&lock)?,"semanticLockSha256":semantic,
              "coverage":coverage::coverage(&load(&graph)?,text(&cell["target"])?)?}),
    )
}

fn work_dirs(work: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut dirs = BTreeMap::new();
    for entry in fs::read_dir(work)? {
        let entry = entry?;
        if entry.path().is_dir() {
            dirs.insert(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "invalid profile path")?,
                entry.path(),
            );
        }
    }
    Ok(dirs)
}

pub fn compare(original: &Path, framework: &Path, archive: &Path, work: &Path) -> Result<()> {
    verify(original, framework, archive)?;
    let release = load(&archive.join("release.json"))?;
    let mut records = BTreeMap::new();
    for cell in list(&release["cells"])? {
        records.insert(key(cell)?, cell);
    }
    let dirs = work_dirs(work)?;
    require(!dirs.is_empty(), "no replay directories")?;
    let mut pins = Pins::new(original, framework)?;
    for (name, dir) in dirs {
        let record = records.get(&name).ok_or("unknown replay profile")?;
        require(
            record["status"] == "candidate",
            format!("no archived candidate for {name}"),
        )?;
        let check = artifact(original, &mut pins, &dir, record)?;
        for field in ["graphSha256", "semanticLockSha256"] {
            require(
                check[field] == record[field],
                format!("replay {field} differs for {name}"),
            )?;
        }
        println!("{name} {}", text(&check["graphSha256"])?);
    }
    Ok(())
}

pub fn seal(original: &Path, framework: &Path, archive: &Path, work: &[PathBuf]) -> Result<()> {
    checkout(framework, FRAMEWORK, false)?;
    let (targets, mut cells) = matrix(original)?;
    let mut observed = BTreeMap::new();
    for dir in work {
        for (k, path) in work_dirs(dir)? {
            require(
                observed.insert(k, path).is_none(),
                "duplicated profile directory",
            )?;
        }
    }
    require(
        archive.symlink_metadata().is_err(),
        "archive already exists",
    )?;
    let parent = archive
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = tempfile::tempdir_in(parent)?;
    let root = tmp.path().join("archive");
    fs::create_dir_all(root.join("graphs"))?;
    fs::create_dir(root.join("locks"))?;
    let mut pins = Pins::new(original, framework)?;
    for cell in &mut cells {
        let name = key(cell)?;
        let Some(dir) = observed.remove(&name) else {
            cell["status"] = "unobserved".into();
            continue;
        };
        let record = artifact(original, &mut pins, &dir, cell)?;
        let digest = text(&record["graphSha256"])?;
        fs::copy(
            dir.join("shapes.json"),
            root.join("graphs").join(format!("{digest}.json")),
        )?;
        fs::copy(
            dir.join("shapes.lock"),
            root.join("locks").join(format!("{name}.json")),
        )?;
        cell.as_object_mut()
            .ok_or("invalid profile")?
            .extend(record.as_object().ok_or("invalid artifact")?.clone());
    }
    require(observed.is_empty(), "unexpected profile directory")?;
    write(
        &root.join("release.json"),
        json!({"version":1,"upstreamRevision":ORIGINAL,"frameworkRevision":FRAMEWORK,
        "vocabulary":VOCAB,"compilerCommit":COMPILER,"cargoProfile":"release","targets":targets,"cells":cells}),
    )?;
    verify(original, framework, &root)?;
    renameat_with(CWD, &root, CWD, archive, RenameFlags::NOREPLACE)?;
    Ok(())
}

fn command(binary: impl AsRef<std::ffi::OsStr>, triple: &str, sysroot: &Path) -> Result<Command> {
    let mut cmd = Command::new(binary);
    for k in [
        "RUSTC",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_BOOTSTRAP",
    ] {
        cmd.env_remove(k);
    }
    cmd.env("RUSTUP_TOOLCHAIN", "1.97.1");
    if triple.ends_with("-linux-gnu") {
        for key in ["LIBRARY_PATH", "LD_LIBRARY_PATH"] {
            let mut dirs = vec![sysroot.join("lib")];
            if let Some(value) = env::var_os(key).filter(|v| !v.is_empty()) {
                dirs.extend(env::split_paths(&value));
            }
            cmd.env(key, env::join_paths(dirs)?);
        }
    }
    Ok(cmd)
}

struct Recovery<'a> {
    original: &'a Path,
    framework: &'a Path,
    work: &'a Path,
    compiler: &'a Path,
    triple: &'a str,
    sysroot: &'a Path,
}
impl Recovery<'_> {
    fn cell(&self, cell: &Value) -> Result<()> {
        let name = key(cell)?;
        let out = self.work.join(&name);
        let mut pins = Pins::new(self.original, self.framework)?;
        if out.exists() {
            artifact(self.original, &mut pins, &out, cell)?;
            println!("{name} (existing)");
            return Ok(());
        }
        let profile = json!({"package":cell["package"],"targetKind":cell["kind"],"targetName":cell["target"],"targetTriple":self.triple,
            "features":cell["features"],"defaultFeatures":cell["defaultFeatures"],"compilerCommit":COMPILER,
            "cargoLockSha256":pins.common["cargoLockSha256"],"adapter":self.framework.join("verification/rust-recovery/run.sh"),
            "adapterInputs":ADAPTER_FILES,"sourceRevision":ORIGINAL});
        let selection = self.work.join(format!("{name}.toml"));
        fs::write(&selection, toml::to_string(&profile)?)?;
        let log_path = self.work.join(format!("{name}.log"));
        let log = fs::File::create(&log_path)?;
        let status = command(self.compiler, self.triple, self.sysroot)?
            .current_dir(self.original)
            .args(["recover", "rust-shapes"])
            .arg(self.original.join("Cargo.toml"))
            .arg("--profile")
            .arg(&selection)
            .arg("--out")
            .arg(&out)
            .stdout(log.try_clone()?)
            .stderr(log)
            .status()?;
        require(
            status.success(),
            format!("{name} recovery failed, see {}", log_path.display()),
        )?;
        artifact(self.original, &mut pins, &out, cell)?;
        println!("{name}");
        Ok(())
    }
}

pub fn recover(
    original: &Path,
    framework: &Path,
    work: &Path,
    triple: &str,
    jobs: usize,
    targets: &[String],
    mode: Option<&str>,
) -> Result<()> {
    checkout(framework, FRAMEWORK, false)?;
    require(jobs > 0, "jobs must be positive")?;
    require(TRIPLES.contains(&triple), "unsupported target triple")?;
    let (_, all) = matrix(original)?;
    let cells: Vec<_> = all
        .into_iter()
        .filter(|c| {
            (targets.is_empty() || targets.iter().any(|t| c["target"] == *t))
                && c["triple"] == triple
                && mode.is_none_or(|m| {
                    c["features"]
                        == if m == "default" {
                            json!([])
                        } else {
                            json!([m])
                        }
                })
        })
        .collect();
    require(
        !cells.is_empty()
            && targets
                .iter()
                .all(|t| cells.iter().any(|c| c["target"] == *t)),
        "unknown target selection",
    )?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    require(
        version
            .lines()
            .any(|l| l.strip_prefix("host: ") == Some(triple)),
        "recovery requires native host",
    )?;
    let sysroot = PathBuf::from(
        stdout(Command::new("rustc").args(["+1.97.1", "--print", "sysroot"]))?.trim(),
    );
    fs::create_dir_all(work)?;
    let build = tempfile::tempdir()?;
    run(command("cargo", triple, &sysroot)?
        .current_dir(framework)
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(build.path()))?;
    let compiler = build.path().join("debug/oddlang");
    let context = Recovery {
        original,
        framework,
        work,
        compiler: &compiler,
        triple,
        sysroot: &sysroot,
    };
    let next = AtomicUsize::new(0);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(cells.len()) {
            scope.spawn(|| {
                while let Some(cell) = cells.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if let Err(err) = context.cell(cell) {
                        errors.lock().unwrap().push(err.to_string());
                    }
                }
            });
        }
    });
    let mut errors = errors.into_inner().unwrap();
    errors.sort();
    require(errors.is_empty(), errors.join("\n"))?;
    checkout(original, ORIGINAL, false)?;
    checkout(framework, FRAMEWORK, false)
}
