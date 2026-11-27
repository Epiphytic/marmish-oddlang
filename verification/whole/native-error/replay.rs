//! Replay the native error caller and the two release-info CLI roles.
mod recover;
use clap::{Parser, Subcommand};
use public_data_qualification::{
    assembly::{bytes_sha, checkout, inventory, stage},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "da0ea5dedd1adfa8513b7f3f8d536713958bcecb";
const MANIFEST: &str = "workspace-v7-native-error.json";

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Recover {
        oddlang: PathBuf,
        original: PathBuf,
        output: PathBuf,
        #[arg(long)]
        original_target: PathBuf,
        #[arg(long)]
        generated_target: PathBuf,
        #[arg(long)]
        framework_revision: String,
    },

    Probe {
        workspace: PathBuf,
        target: PathBuf,
        output: PathBuf,
    },
    Combined {
        framework: PathBuf,
        cache: PathBuf,
        receipt: PathBuf,
    },
}

fn native_probe(workspace: &Path, target: &Path, output: &Path) -> Result<Vec<u8>> {
    fs::create_dir_all(target)?;
    let target = target.canonicalize()?;
    let rows = run(Command::new("cargo")
        .args([
            "+1.97.1",
            "build",
            "--locked",
            "--offline",
            "-p",
            "marmish-native",
            "--lib",
            "--message-format=json",
            "--target-dir",
        ])
        .arg(&target)
        .current_dir(workspace))?;
    let mut libs = BTreeMap::new();
    for line in std::str::from_utf8(&rows)?.lines() {
        let row: Value = serde_json::from_str(line)?;
        if row["reason"] != "compiler-artifact" {
            continue;
        }
        let name = text(&row["target"]["name"])?;
        if !["marmish_native", "marmish_stream"].contains(&name) {
            continue;
        }
        for filename in list(&row["filenames"])? {
            let path = Path::new(text(filename)?);
            if path.extension().is_some_and(|e| e == "rlib") {
                let lib = path.canonicalize()?;
                require(
                    lib.starts_with(&target),
                    "Cargo library is outside selected target",
                )?;
                require(
                    libs.insert(name.to_owned(), lib).is_none(),
                    "duplicate library artifact",
                )?;
            }
        }
    }
    let stream = libs.get("marmish_stream").ok_or("missing stream library")?;
    let native = libs.get("marmish_native").ok_or("missing native library")?;
    let deps = stream.parent().ok_or("missing dependency directory")?;
    let exe = std::env::current_dir()?.join(output);
    run(Command::new("rustc")
        .args(["+1.97.1", "--edition=2024"])
        .arg(whole().join("native-error/probe.rs"))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("marmish_stream={}", stream.display()))
        .arg("--extern")
        .arg(format!("marmish_native={}", native.display()))
        .arg("-o")
        .arg(&exe))?;
    run(&mut Command::new(exe))
}

fn cli_cases(bin_dir: &Path, cases: &Value, runtime: &Path) -> Result<Value> {
    fs::create_dir(runtime)?;
    let mut binaries = json!({});
    require(
        cases.as_object().is_some_and(|obj| obj.len() == 2),
        "unexpected release-info role count",
    )?;
    for role in ["marmish", "marmishd"] {
        let binary = bin_dir.join(role);
        let rows = list(&cases[role])?;
        require(rows.len() == 16, "incomplete release-info cases")?;
        for case in rows {
            let argv = list(&case["argv"])?
                .iter()
                .map(text)
                .collect::<Result<Vec<_>>>()?;
            require(
                argv.contains(&"release-info"),
                "expected a release-info invocation",
            )?;
            let mut cmd = Command::new(&binary);
            cmd.current_dir(runtime)
                .env_clear()
                .env("HOME", runtime)
                .env("TMPDIR", runtime)
                .env("LANG", "C")
                .env("PATH", "/bin:/usr/bin")
                .args(&argv);
            for name in [
                "TARGET",
                "CARGO_PKG_VERSION",
                "MARMISH_BUILD_COMMIT",
                "MARMISH_BUILD_TARGET",
            ] {
                cmd.env(name, "override");
            }
            let out = capture(&mut cmd, 30, None)?;
            require(
                out.status.code().map(i64::from) == case["exit"].as_i64()
                    && out.stdout == text(&case["stdout"])?.as_bytes()
                    && out.stderr == text(&case["stderr"])?.as_bytes(),
                format!("{role} release-info result differs for {argv:?}"),
            )?;
        }
        binaries[role] = json!({"cases":rows.len(),"sha256":sha(&binary)?});
    }
    require(
        fs::read_dir(runtime)?.next().is_none(),
        "release-info created runtime files",
    )?;
    Ok(binaries)
}

fn combined(framework: &Path, cache: &Path) -> Result<Value> {
    let framework = framework.canonicalize()?;
    checkout(&framework, FRAMEWORK, false)?;
    fs::create_dir_all(cache)?;
    let cache = cache.canonicalize()?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing rustc host")?;
    let v7 = load(&whole().join("workspace").join(format!("v7-{host}.json")))?;
    let error = whole().join("native-error");
    let prior = load(&error.join("darwin-arm64/receipt.json"))?;
    let reference = fs::read(error.join("darwin-arm64/baseline.txt"))?;
    let caller = sha(&error.join("probe.rs"))?;
    require(
        prior["profiles"]["baseline"]["originalAndGeneratedOutputSha256"] == bytes_sha(&reference)
            && prior["callerSha256"] == caller,
        "error reference or probe pin differs",
    )?;
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().canonicalize()?;
    let manifest = portable().join(MANIFEST);
    let inputs = root.join("inputs");
    stage(&manifest, &inputs)?;
    let staged = inventory(&inputs)?;
    let compiler = cache.join("compiler");
    run(Command::new("cargo")
        .current_dir(&framework)
        .args([
            "+1.96.1",
            "build",
            "--bin",
            "oddlang",
            "--offline",
            "--locked",
            "--target-dir",
        ])
        .arg(&compiler))?;
    let product = root.join("generated");
    run(Command::new(compiler.join("debug/oddlang"))
        .current_dir(&root)
        .env("RUSTUP_TOOLCHAIN", "1.97.1")
        .env("CARGO_NET_OFFLINE", "true")
        .args(["build", "native-workspace", "rust"])
        .arg(inputs.join(MANIFEST))
        .arg(&product))?;
    let target = cache.join("app");
    run(Command::new("cargo")
        .current_dir(&product)
        .args([
            "+1.97.1",
            "build",
            "--release",
            "--workspace",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&target))?;
    let output = native_probe(&product, &target, &root.join("caller"))?;
    require(
        output == reference,
        "native error output differs from reference",
    )?;
    let binaries = cli_cases(
        &target.join("release"),
        &v7["releaseInfo"]["cases"],
        &root.join("runtime"),
    )?;
    Ok(json!({
        "framework":FRAMEWORK,
        "rustc":rust.lines().collect::<Vec<_>>(),
        "manifestSha256":sha(&manifest)?,
        "workspaceLockSha256":sha(&product.join("workspace.lock"))?,
        "binaries":binaries,
        "referenceBinaries":v7["releaseInfo"]["originalBinaries"],
        "callerSha256":caller,
        "callerOutputSha256":bytes_sha(&output),
        "callerRows":std::str::from_utf8(&output)?.lines().count(),
        "stagedInputs":staged
    }))
}

fn main() -> Result {
    match Args::parse().command {
        Mode::Recover {
            oddlang,
            original,
            output,
            original_target,
            generated_target,
            framework_revision,
        } => recover::qualify(
            &oddlang,
            &original,
            &output,
            &original_target,
            &generated_target,
            &framework_revision,
        )?,

        Mode::Probe {
            workspace,
            target,
            output,
        } => io::stdout()
            .lock()
            .write_all(&native_probe(&workspace, &target, &output)?)?,
        Mode::Combined {
            framework,
            cache,
            receipt,
        } => fs::write(receipt, json_bytes(&combined(&framework, &cache)?)?)?,
    }
    Ok(())
}
