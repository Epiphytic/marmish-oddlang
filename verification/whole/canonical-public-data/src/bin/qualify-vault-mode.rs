//! Qualify the public vault enum against original and generated libraries.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{self, ORIGINAL, Sandbox},
    build_host::{self, cargo_command},
    capture, json_bytes, load, portable, require, run, sha, stdout,
    vault_mode::observe,
    whole,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "18ea58006de4482c0ebc61de9e800edc78eba987";
const SOURCE: &str = "crates/marmish-vault/src/helper.rs";
const DOMAIN: &str = "vault-mode-v1";
const MANIFEST: &str = "vault-fixed-value-mode-package.json";

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    compiler_target: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
}

fn cargo(home: &Path, dir: &Path) -> Command {
    cargo_command(home, dir, env::vars_os().map(|(k, _)| k))
}

fn original_library(home: &Path, checkout: &Path, target: &Path, host: &str) -> Result<PathBuf> {
    assembly::reject_local_cargo_config(checkout)?;
    let built = capture(
        cargo(home, checkout)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--target",
                host,
                "--target-dir",
            ])
            .arg(target)
            .args(["-p", "marmish-vault", "--lib", "--message-format=json"]),
        1800,
        None,
    )?;
    require(
        built.status.success(),
        String::from_utf8_lossy(&built.stderr),
    )?;
    let manifest = checkout.join("crates/marmish-vault/Cargo.toml");
    let mut libraries = Vec::new();
    for line in std::str::from_utf8(&built.stdout)?.lines() {
        let event: Value = serde_json::from_str(line)?;
        if event["reason"] == "compiler-artifact"
            && event["target"]["kind"] == json!(["lib"])
            && Path::new(
                event["manifest_path"]
                    .as_str()
                    .ok_or("missing Cargo manifest")?,
            )
            .canonicalize()?
                == manifest
        {
            for f in event["filenames"]
                .as_array()
                .ok_or("missing Cargo outputs")?
            {
                let p = Path::new(f.as_str().ok_or("invalid Cargo filename")?);
                if p.extension().is_some_and(|e| e == "rlib") {
                    libraries.push(p.canonicalize()?);
                }
            }
        }
    }
    let [lib] = libraries.as_slice() else {
        return Err("expected exactly one vault library".into());
    };
    require(
        lib.starts_with(target.join(host)),
        "unexpected vault library location",
    )?;
    Ok(lib.clone())
}

fn stage(inputs: &Path, recovery: &Path, baseline: bool) -> Result<PathBuf> {
    let base = portable();
    let mut spec = load(&base.join(MANIFEST))?;
    assembly::pinned_copy(&base, &spec["cargoLock"], inputs)?;
    for c in spec["components"]
        .as_array_mut()
        .ok_or("missing package components")?
    {
        for key in ["graph", "overlay"] {
            let src = assembly::pinned_copy(&base, &c[key], inputs)?;
            let relative = src.strip_prefix(base.canonicalize()?)?;
            if relative.parent() == Some(Path::new(DOMAIN)) {
                let replacement =
                    recovery.join(src.file_name().ok_or("missing recovered filename")?);
                if baseline {
                    require(
                        fs::read(&replacement)? == fs::read(&src)?,
                        "recovered input differs from published bytes",
                    )?;
                }
                c[key]["sha256"] = json!(sha(&replacement)?);
                fs::copy(replacement, inputs.join(relative))?;
            }
        }
    }
    let out = inputs.join(MANIFEST);
    fs::write(&out, json_bytes(&spec)?)?;
    Ok(out)
}

fn main() -> Result {
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let target = assembly::resolved(&std::path::absolute(&args.original_target)?)?;
    let compiler_target = assembly::resolved(&std::path::absolute(&args.compiler_target)?)?;
    let check = || -> Result {
        assembly::checkout(&framework, FRAMEWORK, true)?;
        assembly::checkout(&original, ORIGINAL, true)
    };
    check()?;
    let version = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("unknown native toolchain host")?;
    require(
        [assembly::DARWIN, assembly::LINUX].contains(&host),
        "unsupported host",
    )?;
    let temp = tempfile::tempdir()?;
    let scratch = temp.path().canonicalize()?;
    let isolated = tempfile::tempdir()?;
    let work = isolated.path().canonicalize()?;
    let home = scratch.join("cargo-home");
    build_host::prepare(&home)?;
    run(cargo(&home, &framework)
        .args([
            "+1.96.1",
            "build",
            "--bin",
            "oddlang",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&compiler_target))?;
    check()?;
    let compiler = work.join("oddlang");
    fs::copy(compiler_target.join("debug/oddlang"), &compiler)?;
    fs::create_dir_all(&target)?;
    let git_cache = work.join("empty-git-cache");
    fs::create_dir(&git_cache)?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[assembly::root(), &original, &framework, &target, &scratch],
        &git_cache,
    )?;
    sandbox.deny(&[
        original.join(SOURCE),
        whole().join("vault-mode/probe.rs"),
        framework.join("src/lib.rs"),
    ])?;
    let mutant = scratch.join("mutant");
    assembly::archive(&original, ORIGINAL, &mutant)?;
    let source = mutant.join(SOURCE);
    let old = "pub enum AuthorizationMode {\n    Interactive,\n    Verify,\n}";
    let new = "pub enum AuthorizationMode {\n    Verify,\n    Interactive,\n}";
    let body = fs::read_to_string(&source)?;
    require(
        body.matches(old).count() == 1,
        "expected unique variant declaration",
    )?;
    fs::write(&source, body.replace(old, new))?;
    let mut results = json!({});
    for (name, checkout, expected) in [
        ("baseline", &original, ["interactive=0", "verify=1"]),
        ("mutation", &mutant, ["interactive=1", "verify=0"]),
    ] {
        let baseline = name == "baseline";
        let target = target.join(name);
        let lib = original_library(&home, checkout, &target, host)?;
        let digest = sha(&lib)?;
        let reference = observe(&lib, &scratch, &target.join("release/deps"))?;
        let source = checkout.join(SOURCE);
        let source_sha = sha(&source)?;
        let mut selector = whole().join("vault-mode/selection.json");
        let mut spec = load(&selector)?;
        if baseline {
            require(
                spec["sourceSha256"] == source_sha,
                "different original helper",
            )?;
        } else {
            spec["sourceSha256"] = json!(source_sha);
            selector = scratch.join("mutated-selection.json");
            fs::write(&selector, serde_json::to_vec(&spec)?)?;
        }
        let recovery = scratch.join(name);
        run(Command::new(&compiler)
            .args(["recover", "plain-choice"])
            .arg(&source)
            .arg(&selector)
            .arg(&recovery))?;
        let domain = portable().join(DOMAIN);
        if baseline {
            require(
                fs::read(recovery.join("observation.json"))?
                    == fs::read(domain.join("observation.json"))?,
                "recovery observation differs",
            )?;
        }
        let inputs = work.join(format!("{name}-inputs"));
        let manifest = stage(&inputs, &recovery, baseline)?;
        if baseline {
            let checked = work.join("obligations");
            run(sandbox
                .command(&work)?
                .arg(&compiler)
                .args(["build", "checked-values", "rust"])
                .arg(inputs.join(DOMAIN).join("graph.json"))
                .arg(inputs.join(DOMAIN).join("rust.json"))
                .arg(&checked))?;
            require(
                fs::read(checked.join("obligations.json"))?
                    == fs::read(domain.join("obligations.json"))?,
                "published obligations differ",
            )?;
        }
        let product = work.join(format!("{name}-package"));
        run(sandbox
            .command(&work)?
            .arg(&compiler)
            .args(["build", "native-package", "rust"])
            .arg(manifest)
            .arg(&product))?;
        let lock = sha(&product.join("package.lock"))?;
        if baseline {
            require(
                lock == sha(&domain.join("package.lock"))?,
                "baseline package lock differs",
            )?;
        }
        run(sandbox.command(&product)?.args([
            "cargo",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--lib",
        ]))?;
        let library = product.join("target/release/libmarmish_vault.rlib");
        let actual = observe(&library, &scratch, &product.join("target/release/deps"))?;
        require(
            actual == reference && actual["values"] == json!(expected),
            "original and generated mode observations differ",
        )?;
        results[name] = json!({"sourceSha256":source_sha,"graphSha256":sha(&recovery.join("graph.json"))?,"overlaySha256":sha(&recovery.join("rust.json"))?,"packageLockSha256":lock,
            "originalLibrarySha256":digest,"generatedLibrarySha256":sha(&library)?,"values":actual["values"],"absentTraits":actual["absentTraits"]});
    }
    require(
        results["baseline"]["graphSha256"] != results["mutation"]["graphSha256"],
        "mutation did not reach portable graph",
    )?;
    check()?;
    fs::write(
        args.receipt,
        json_bytes(
            &json!({"original":ORIGINAL,"host":host,"compiler":version.lines().collect::<Vec<_>>(),"framework":FRAMEWORK,
        "compilerSha256":sha(&compiler)?,"probeSha256":sha(&whole().join("vault-mode/probe.rs"))?,"results":results}),
        )?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn build_environment_uses_only_dependency_caches() -> Result {
        let temp = tempfile::tempdir()?;
        let ambient = temp.path().join("ambient");
        let home = temp.path().join("private");
        fs::create_dir_all(ambient.join("registry"))?;
        fs::create_dir(ambient.join("git"))?;
        fs::write(ambient.join("config.toml"), "[build]\njobs = 7\n")?;
        build_host::private_home(&home, &ambient)?;
        assert_eq!(fs::read_dir(&home)?.count(), 2);
        for name in ["git", "registry"] {
            assert_eq!(
                home.join(name).canonicalize()?,
                ambient.join(name).canonicalize()?
            );
        }
        let keys = [
            "CARGO_HOME",
            "CARGO_PROFILE_RELEASE_OPT_LEVEL",
            "RUSTC_WRAPPER",
            "RUSTDOCFLAGS",
            "RUSTFLAGS",
        ];
        let command = cargo_command(&home, temp.path(), keys.map(OsString::from).into_iter());
        let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        for key in &keys[1..] {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None));
        }
        assert_eq!(
            env.get(std::ffi::OsStr::new("CARGO_HOME")),
            Some(&Some(home.as_os_str()))
        );
        Ok(())
    }

    #[test]
    fn checkout_rejects_config_dirty_files_and_revision_changes() -> Result {
        let temp = tempfile::tempdir()?;
        let repo = temp.path().join("repo");
        fs::create_dir(&repo)?;
        let git = |args: &[&str]| stdout(Command::new("git").current_dir(&repo).args(args));
        git(&["init", "-q"])?;
        let commit = [
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "test",
        ];
        git(&commit)?;
        let rev = git(&["rev-parse", "HEAD"])?;
        let check = || assembly::checkout(&repo, rev.trim(), true);
        check()?;
        for path in [
            repo.join(".cargo/config.toml"),
            temp.path().join(".cargo/config"),
        ] {
            fs::create_dir(path.parent().unwrap())?;
            fs::write(&path, "")?;
            assert!(
                check()
                    .unwrap_err()
                    .to_string()
                    .contains("Cargo configuration")
            );
            fs::write(repo.join(".git/info/exclude"), ".cargo/\n")?;
            assert!(
                check()
                    .unwrap_err()
                    .to_string()
                    .contains("Cargo configuration")
            );
            fs::remove_file(path)?;
        }
        fs::write(repo.join("untracked"), "")?;
        assert!(check().is_err());
        fs::remove_file(repo.join("untracked"))?;
        check()?;
        git(&commit)?;
        assert!(check().is_err());
        Ok(())
    }
}
