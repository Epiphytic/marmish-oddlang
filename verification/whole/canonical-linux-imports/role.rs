//! Build and probe one of the original or generated native library sets.
use public_data_qualification::{
    Result,
    artifacts::{cargo_rows, library_closure, selected_libraries},
    assembly::{DARWIN, LINUX, bytes_sha},
    imports, list, require, text,
};
use serde_json::{Value, json};
use std::{env, path::Path, process::Command};

fn build_command(root: &Path, target: &Path, host: &str) -> Result<Command> {
    require(
        matches!(host, DARWIN | LINUX),
        "unsupported public import host",
    )?;
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root).env_clear();
    for key in ["HOME", "PATH", "CARGO_HOME", "TMPDIR", "SDKROOT"] {
        if (key != "SDKROOT" || host == DARWIN)
            && let Some(value) = env::var_os(key)
        {
            cmd.env(key, value);
        }
    }
    cmd.env("CARGO_NET_OFFLINE", "true")
        .env("LC_ALL", "C")
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--workspace",
            "--lib",
            "--locked",
            "--offline",
        ]);
    if host == LINUX {
        cmd.args(["--target", host]);
    }
    cmd.arg("--target-dir")
        .arg(target)
        .arg("--message-format=json");
    Ok(cmd)
}

pub fn build(root: &Path, target: &Path, host: &str) -> Result<String> {
    let output = build_command(root, target, host)?.output()?;
    require(
        output.status.success(),
        format!(
            "public library build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?)
}

pub fn report(
    raw: &str,
    root: &Path,
    target: &Path,
    original: &Path,
    host: &str,
    generated: bool,
) -> Result<Value> {
    let release = match host {
        LINUX => target.join(host).join("release"),
        DARWIN => target.join("release"),
        _ => return Err("unsupported public import host".into()),
    };
    let packages = imports::COUNT.map(|(name, _)| format!("marmish-{name}"));
    let rows = cargo_rows(raw.as_bytes())?;
    let package_root = if generated {
        root.to_owned()
    } else {
        root.join("crates")
    }
    .canonicalize()?;
    let target = target.canonicalize()?;
    let (roles, _) = selected_libraries(
        &rows,
        &package_root,
        &target,
        &packages.each_ref().map(String::as_str),
    )?;
    let release = release.canonicalize()?;
    for role in roles
        .as_object()
        .ok_or("missing selected libraries")?
        .values()
    {
        for file in &list(role)?[..2] {
            require(
                Path::new(text(file)?).starts_with(&release),
                "library outside native release directory",
            )?;
        }
    }
    let before = library_closure(&rows, &target, host)?;
    let libraries = imports::library_set(host, original, &roles)?;
    let work = tempfile::tempdir()?;
    let mut controls = json!({});
    for (label, item, code) in [
        ("absent", "NoSuchAuditSentinel", "E0432"),
        ("private", "config", "E0603"),
    ] {
        let result = imports::probe(
            host,
            &roles["marmish-core"],
            work.path(),
            &format!("pub use observed::{item} as witness;"),
        )?;
        let diagnostics = list(&result["diagnostics"])?;
        require(
            result["compiles"] == false && diagnostics.len() == 1 && diagnostics[0]["code"] == code,
            format!("unexpected diagnostic for {label} core import"),
        )?;
        controls[label] = result;
    }
    require(
        library_closure(&rows, &target, host)? == before,
        "Cargo libraries changed during public import probes",
    )?;
    Ok(
        json!({"cargoArtifactLogSha256":bytes_sha(raw.as_bytes()),"cargoArtifactClosure":before,"negativeImportControls":controls,"libraries":libraries}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_build_selects_the_release_layout_for_each_host() -> Result {
        let path = Path::new("/tmp");
        for host in [DARWIN, LINUX] {
            let cmd = build_command(path, path, host)?;
            let args = cmd.get_args().collect::<Vec<_>>();
            assert_eq!(
                args.windows(2).any(|a| a == ["--target", host]),
                host == LINUX
            );
            assert!(args.iter().any(|a| *a == "--offline"));
            assert!(args.iter().any(|a| *a == "--locked"));
        }
        assert!(build_command(path, path, "unknown").is_err());
        Ok(())
    }
}
