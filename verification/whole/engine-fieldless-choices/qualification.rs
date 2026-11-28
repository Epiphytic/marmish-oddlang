//! Qualify the original and regenerated libraries with the same external Rust caller.
use super::{DOMAIN, MODEL, PACKAGE, mutate, original, prepare, recover, write_file};
use public_data_qualification::{
    Result,
    artifacts::cargo_rows,
    assembly::{self, ORIGINAL, Sandbox},
    build_host, json_bytes, list, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "8e3320aacb0106d543c3e21ab1faf60a02132c04";
const PREDECESSOR: &str = "engine-observational-records-package.json";
#[derive(clap::Args)]
pub struct Args {
    input_revision: String,
    framework: PathBuf,
    original: PathBuf,
    compiler_target: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
}

fn engine_library(rows: &[Value], manifest: &Path, target: &Path) -> Result<PathBuf> {
    let manifest = manifest.canonicalize()?;
    let mut found = Vec::new();
    for row in rows {
        if row["reason"] == "compiler-artifact"
            && row["target"]["name"] == "marmish_engine"
            && row["target"]["kind"] == json!(["lib"])
            && row["profile"]["opt_level"] == "3"
            && row["profile"]["test"] == false
            && Path::new(text(&row["manifest_path"])?).canonicalize()? == manifest
        {
            for file in list(&row["filenames"])? {
                let path = Path::new(text(file)?);
                if path.extension().is_some_and(|v| v == "rlib") {
                    found.push(path.canonicalize()?);
                }
            }
        }
    }
    let [library] = found.as_slice() else {
        return Err("expected one native engine library".into());
    };
    require(
        library.is_file() && library.starts_with(target.canonicalize()?),
        "unexpected native engine library location",
    )?;
    Ok(library.clone())
}

pub struct Reference {
    library: PathBuf,
    serde: PathBuf,
    links: BTreeSet<String>,
}

impl Reference {
    pub fn as_json(&self) -> Value {
        json!({"library":self.library,"serde":self.serde,"links":self.links})
    }
}

pub fn reference(
    mut cargo: Command,
    checkout: &Path,
    host: &str,
    target: &Path,
) -> Result<Reference> {
    build_host::reject_local_cargo_config(checkout)?;
    let bytes = run(cargo
        .current_dir(checkout)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--lib",
            "--target",
            host,
            "--target-dir",
        ])
        .arg(target)
        .args(["-p", "marmish-engine", "--message-format=json"]))?;
    let rows = cargo_rows(&bytes)?;
    let library = engine_library(
        &rows,
        &checkout.join("crates/marmish-engine/Cargo.toml"),
        target,
    )?;
    let release = target.join(host).join("release").canonicalize()?;
    let mut serde = Vec::new();
    let mut links = BTreeSet::new();
    for row in &rows {
        if row["reason"] == "build-script-executed" {
            for link in list(&row["linked_paths"])? {
                links.insert(text(link)?.to_owned());
            }
        }
        if row["reason"] == "compiler-artifact"
            && row["target"]["name"] == "serde"
            && row["target"]["kind"] == json!(["lib"])
        {
            for file in list(&row["filenames"])? {
                let path = Path::new(text(file)?);
                if path.extension().is_some_and(|e| e == "rlib") && path.starts_with(&release) {
                    let resolved = path.canonicalize()?;
                    require(
                        resolved.is_file() && resolved.starts_with(&release),
                        "serde library outside target release",
                    )?;
                    serde.push(resolved);
                }
            }
        }
    }
    let [serde] = serde.as_slice() else {
        return Err("expected one target serde library".into());
    };
    Ok(Reference {
        library,
        serde: serde.clone(),
        links,
    })
}

pub fn qualify(args: Args) -> Result {
    let root = assembly::root().canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let upstream = args.original.canonicalize()?;
    let compiler_target = assembly::resolved(&std::path::absolute(args.compiler_target)?)?;
    let upstream_target = assembly::resolved(&std::path::absolute(args.original_target)?)?;
    let check = || -> Result {
        for (path, rev) in [
            (&root, args.input_revision.as_str()),
            (&framework, FRAMEWORK),
            (&upstream, ORIGINAL),
        ] {
            assembly::checkout(path, rev, true)?;
            require(
                !compiler_target.starts_with(path) && !upstream_target.starts_with(path),
                "expected external Cargo targets",
            )?;
        }
        Ok(())
    };
    check()?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let native = match env::consts::OS {
        "macos" => assembly::DARWIN,
        "linux" => assembly::LINUX,
        _ => return Err("unsupported platform".into()),
    };
    require(
        host == native
            && env::consts::ARCH == "aarch64"
            && version
                .lines()
                .any(|s| s.strip_prefix("commit-hash: ") == Some(assembly::COMPILER)),
        "requires pinned native ARM64 toolchain",
    )?;
    let temp = tempfile::tempdir()?;
    let isolated = tempfile::tempdir()?;
    let scratch = temp.path().canonicalize()?;
    let work = isolated.path().canonicalize()?;
    let home = scratch.join("cargo-home");
    build_host::prepare(&home)?;
    let cargo = |dir: &Path| build_host::cargo_command(&home, dir, env::vars_os().map(|(k, _)| k));
    run(cargo(&framework)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    let compiler = work.join("oddlang");
    fs::copy(compiler_target.join("release/oddlang"), &compiler)?;
    let model = original(&upstream)?;
    for (rel, expected) in recover(&compiler, &model)?.files {
        require(
            fs::read(portable().join(rel))? == expected,
            "published engine package inputs differ",
        )?;
    }
    let providers = work.join("providers");
    fs::create_dir(&providers)?;
    fs::create_dir_all(&upstream_target)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&root, &upstream, &framework, &upstream_target, &scratch],
        &providers,
    )?;
    let lane = whole().join("engine-fieldless-choices");
    sandbox.deny(&[
        upstream.join(MODEL),
        framework.join("src/lib.rs"),
        lane.join("probe.rs"),
    ])?;
    let mutant = scratch.join("mutation");
    assembly::archive(&upstream, ORIGINAL, &mutant)?;
    fs::write(mutant.join(MODEL), mutate(model)?)?;
    let mut scenarios = json!({});
    for (name, source, is_mutant) in [("baseline", &upstream, false), ("mutation", &mutant, true)] {
        let inputs = work.join(format!("{name}-inputs"));
        let model = source.join(MODEL);
        let recovery = prepare(&compiler, &model, &inputs)?;
        let product = work.join(format!("{name}-engine"));
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(inputs.join(PACKAGE))
            .arg(&product))?;
        let lock = sha(&product.join("package.lock"))?;
        if !is_mutant {
            require(
                lock == sha(&portable().join(DOMAIN).join("package.lock"))?,
                "published package lock differs",
            )?;
        }
        let target = work.join(format!("{name}-target"));
        let manifest = product.join("Cargo.toml");
        let built = run(sandbox
            .command(&work)?
            .args([
                "cargo",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--lib",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .arg("--message-format=json"))?;
        let generated = engine_library(&cargo_rows(&built)?, &manifest, &target)?;
        let reference_target = upstream_target.join(name);
        let reference = reference(cargo(source), source, host, &reference_target)?;
        let observe = |library: &Path, links: &BTreeSet<String>, role: &str| -> Result<Value> {
            let mut cmd = Command::new(env::current_exe()?);
            for (key, value) in cargo(source).get_envs() {
                if let Some(v) = value {
                    cmd.env(key, v);
                } else {
                    cmd.env_remove(key);
                }
            }
            cmd.arg("observe")
                .arg(library)
                .arg(&reference.serde)
                .arg(reference_target.join("release/deps"))
                .arg(host)
                .arg(scratch.join(format!("{name}-{role}")));
            if is_mutant {
                cmd.arg("--copy-disposition");
            }
            for link in links {
                cmd.arg("--link").arg(link);
            }
            Ok(serde_json::from_slice(&run(&mut cmd)?)?)
        };
        let expected = observe(&reference.library, &reference.links, "original")?;
        let actual = observe(&generated, &BTreeSet::new(), "generated")?;
        let values: Vec<_> = [
            ("Stream", "Stdout"),
            ("Stream", "Stderr"),
            ("SubmitDisposition", "Accepted"),
            ("SubmitDisposition", "Existing"),
        ]
        .iter()
        .enumerate()
        .map(|(i, (kind, value))| {
            format!(
                "{kind}:{value}:{value}:{}",
                (i % 2) ^ usize::from(is_mutant)
            )
        })
        .collect();
        require(
            actual == expected && actual["values"] == json!(values),
            "native enum witnesses differ",
        )?;
        scenarios[name] = json!({"modelSha256":sha(&model)?,"recovery":recovery,"graphSha256":sha(&inputs.join(DOMAIN).join("graph.json"))?,
            "bindingSha256":sha(&inputs.join(DOMAIN).join("rust.json"))?,"enginePackageLockSha256":lock,"originalLibrarySha256":sha(&reference.library)?,
            "generatedLibrarySha256":sha(&generated)?,"values":actual["values"],"traits":actual["traits"]});
    }
    require(
        scenarios["baseline"]["graphSha256"] != scenarios["mutation"]["graphSha256"],
        "mutation did not reach graph",
    )?;
    check()?;
    let receipt = json!({"platform":host,"original":ORIGINAL,"inputRevision":args.input_revision,"framework":FRAMEWORK,
        "compilerSha256":sha(&compiler)?,"rustc":version.lines().collect::<Vec<_>>(),"probeSha256":sha(&lane.join("probe.rs"))?,
        "verifierSha256":sha(&env::current_exe()?)?,"predecessorManifestSha256":sha(&portable().join(PREDECESSOR))?,"scenarios":scenarios});
    write_file(&std::path::absolute(args.receipt)?, &json_bytes(&receipt)?)
}
