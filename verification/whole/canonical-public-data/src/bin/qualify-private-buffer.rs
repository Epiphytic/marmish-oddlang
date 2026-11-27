//! Recover the original private reader and qualify the sealed package and four controls.
use clap::Parser;
use public_data_qualification::{
    assembly::{ORIGINAL, bytes_sha, checkout},
    buffered::{CASES, READER_TEST_SHA, SOURCE, original_test, provider, reader_source},
    private_buffer::{MANIFEST, capture as observe, project},
    *,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    compiler_cache: PathBuf,
    receipt: PathBuf,
}
const PREFIX: &str = "buffer::__projection_0::";
const BUNDLE: &str = "prototype-buffer-v1";

// Historical control manifests use the Python JSON encoder's default separators.
fn control_json(value: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut string = false;
    let mut escaped = false;
    for b in serde_json::to_vec(value)? {
        out.push(b);
        if string && escaped {
            escaped = false;
        } else if string && b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            string = !string;
        } else if !string && (b == b',' || b == b':') {
            out.push(b' ');
        }
    }
    Ok(out)
}
struct Suite {
    original: PathBuf,
    framework: PathBuf,
    cli: PathBuf,
    host: String,
    root: PathBuf,
    module: String,
}

fn test(product: &Path, case: &str, failure: Option<&str>) -> Result<i32> {
    let exact = format!("{PREFIX}{case}");
    let r = capture(
        Command::new("cargo").current_dir(product).args([
            "+1.97.1",
            "test",
            "--release",
            "--offline",
            "--locked",
            "--lib",
            "--",
            "--exact",
            &exact,
        ]),
        600,
        None,
    )?;
    let out = String::from_utf8(r.stdout)?;
    let (exit, status) = if failure.is_some() {
        (101, "FAILED")
    } else {
        (0, "ok")
    };
    require(
        r.status.code() == Some(exit)
            && out
                .lines()
                .filter(|l| *l == format!("test {exact} ... {status}"))
                .count()
                == 1
            && failure.is_none_or(|msg| out.contains(msg)),
        format!(
            "unexpected observation {exact}\n{out}\n{}",
            String::from_utf8_lossy(&r.stderr)
        ),
    )?;
    Ok(exit)
}
fn width(product: &Path) -> Result {
    let r = capture(
        Command::new("cargo").current_dir(product).args([
            "+1.97.1",
            "test",
            "--release",
            "--locked",
            "--offline",
            "--no-run",
            "--lib",
            "--message-format=json",
        ]),
        600,
        None,
    )?;
    let mut errors = vec![];
    for line in std::str::from_utf8(&r.stdout)?.lines() {
        let row: Value = serde_json::from_str(line)?;
        if row["reason"] == "compiler-message" && row["message"]["level"] == "error" {
            errors.push(row["message"].clone());
        }
    }
    let [error] = errors.as_slice() else {
        return Err(format!("expected one width diagnostic: {errors:?}").into());
    };
    let expected = list(&error["spans"])?
        .iter()
        .filter(|s| s["is_primary"] == true)
        .filter_map(|s| s["text"].as_array())
        .flatten()
        .any(|t| {
            t["text"]
                .as_str()
                .is_some_and(|l| l.contains("Zeroizing::new([0; 8192])"))
        });
    require(
        r.status.code() == Some(101)
            && error["code"]["code"] == "E0308"
            && text(&error["rendered"])?.contains("4096")
            && expected,
        "wrong width diagnostic or source span",
    )
}
impl Suite {
    fn recover(&self, stage: &Path, src: &Path) -> Result<PathBuf> {
        let obs = stage.join("observation.json");
        fs::write(&obs, observe(&self.framework, src, &self.host)?)?;
        let out = stage.join("recovered");
        run(Command::new(&self.cli)
            .args(["recover", "buffered-input"])
            .arg(obs)
            .arg(whole().join("private-buffer/selection.json"))
            .arg(&out))?;
        Ok(out)
    }
    fn lane(&self, name: &str, graph: &[u8]) -> Result<Value> {
        let stage = self.root.join(name);
        let temp = if cfg!(target_os = "linux") {
            tempfile::tempdir_in("/tmp")?
        } else {
            tempfile::tempdir()?
        };
        let sealed = temp.path().canonicalize()?;
        let inputs = sealed.join("inputs");
        fs::create_dir(&inputs)?;
        let base = portable();
        let manifest = base.join(MANIFEST);
        let mut spec = load(&manifest)?;
        let [component] = list(&spec["components"])?.as_slice() else {
            return Err("expected one private buffer component".into());
        };
        for pin in [
            &spec["cargoLock"],
            &component["graph"],
            &component["overlay"],
        ] {
            let rel = text(&pin["path"])?;
            let data = fs::read(base.join(rel))?;
            require(
                bytes_sha(&data) == pin["sha256"],
                format!("input pin changed: {rel}"),
            )?;
            let dst = inputs.join(rel);
            fs::create_dir_all(dst.parent().ok_or("missing input directory")?)?;
            fs::write(dst, data)?;
        }
        let g = &mut spec["components"][0]["graph"];
        fs::write(inputs.join(text(&g["path"])?), graph)?;
        g["sha256"] = bytes_sha(graph).into();
        fs::write(
            inputs.join(MANIFEST),
            if name == "reference" {
                fs::read(manifest)?
            } else {
                control_json(&spec)?
            },
        )?;
        let mut denied = vec![self.root.join("reference/observation.json")];
        let alternate = self.root.join("mutant-checkout").join(SOURCE);
        if alternate.exists() {
            denied.push(alternate);
        }
        let (built, lock) = project(
            &sealed,
            &inputs,
            &self.cli,
            &self.original,
            &self.framework,
            &self.host,
            &denied,
        )?;
        let product = stage.join("product");
        // The Linux sandbox requires a direct /tmp child, which can be on a different filesystem.
        run(Command::new("mv").arg(&built).arg(&product))?;
        drop(temp);
        let path = product.join("src/group_0/__projection_0.rs");
        let mut out = fs::OpenOptions::new().append(true).open(path)?;
        out.write_all(self.module.as_bytes())?;
        if name != "changed-source" {
            out.write_all(&fs::read(whole().join("private-buffer/observations.rs"))?)?;
        }
        drop(out);
        let mut result = json!({"packageLockSha256":lock});
        match name {
            "reference" => {
                for case in CASES {
                    test(&product, case, None)?;
                }
                result["passed"] = json!(CASES);
                result["generatedPrefix"] = PREFIX.into();
            }
            "changed-source" => {
                width(&product)?;
                result["originalTestDiagnostic"] = "E0308".into();
                result["expectedExtent"] = 4096.into();
            }
            "changed-policy" => {
                result["exit"] =
                    test(&product, CASES[2], Some("failed refill preserves old end"))?.into()
            }
            "wipe-policy" => {
                result["holdoutExit"] =
                    test(&product, CASES[1], Some("consumed span must be erased"))?.into();
                result["originalExit"] =
                    test(&product, CASES[0], Some("input.bytes[..8].iter().all"))?.into();
            }
            "empty-policy" => {
                result["exit"] = test(
                    &product,
                    CASES[3],
                    Some("empty destination must still poll exhausted input"),
                )?
                .into()
            }
            _ => return Err("unknown buffer scenario".into()),
        }
        Ok(result)
    }
    fn verify(&self, source: &str) -> Result<Value> {
        let here = whole().join("private-buffer");
        let upstream = load(&here.join(format!("original-{}.json", self.host)))?;
        let witness = sha(&here.join("observations.rs"))?;
        require(
            upstream["revision"] == ORIGINAL
                && upstream["target"] == self.host
                && upstream["passed"] == json!(CASES)
                && upstream["upstreamFileSha256"] == bytes_sha(source.as_bytes())
                && upstream["callerSha256"] == witness,
            "upstream witness or original source changed",
        )?;
        let provider = provider(&self.original, &self.host)?;
        for n in [
            "reference",
            "changed-source",
            "changed-policy",
            "wipe-policy",
            "empty-policy",
        ] {
            fs::create_dir(self.root.join(n))?;
        }
        let baseline = self.recover(&self.root.join("reference"), &self.original)?;
        let graph = fs::read(baseline.join("graph.json"))?;
        let overlay = portable().join(BUNDLE).join("rust.json");
        require(
            graph == fs::read(portable().join(BUNDLE).join("graph.json"))?
                && fs::read(baseline.join("rust.json"))? == fs::read(&overlay)?,
            "baseline recovery drift",
        )?;
        let normal = self.lane("reference", &graph)?;
        let mutant = self.root.join("mutant-checkout");
        run(Command::new("git")
            .args(["clone", "-q", "--local", "--no-hardlinks", "--no-checkout"])
            .arg(&self.original)
            .arg(&mutant))?;
        run(Command::new("git")
            .current_dir(&mutant)
            .args(["checkout", "-q", "--detach", ORIGINAL]))?;
        let src = mutant.join(SOURCE);
        let raw = fs::read_to_string(&src)?;
        require(
            raw.matches("[u8; 8192]").count() == 1 && raw.matches("[0; 8192]").count() == 2,
            "extent mutation anchors changed",
        )?;
        fs::write(
            src,
            raw.replace("[u8; 8192]", "[u8; 4096]")
                .replace("[0; 8192]", "[0; 4096]"),
        )?;
        let changed = self.recover(&self.root.join("changed-source"), &mutant)?;
        let smaller = fs::read(changed.join("graph.json"))?;
        let reference: Value = serde_json::from_slice(&graph)?;
        let mut expect = reference.clone();
        expect["capacity"] = 4096.into();
        require(
            serde_json::from_slice::<Value>(&smaller)? == expect
                && fs::read(changed.join("rust.json"))? == fs::read(&overlay)?,
            "source recovery changed beyond the declared capacity",
        )?;
        let extent = self.lane("changed-source", &smaller)?;
        let policy = |name, setting: &str, value: &str| {
            let mut policy = reference.clone();
            policy["transitions"][setting] = value.into();
            self.lane(name, &control_json(&policy)?)
        };
        let refill = policy("changed-policy", "refillError", "discard-window")?;
        let wipe = policy("wipe-policy", "consumedBytes", "retain")?;
        let empty = policy("empty-policy", "emptyRead", "return-zero")?;
        Ok(
            json!({"target":self.host,"original":ORIGINAL,"additionalPolicyControls":{"consumedBytes":wipe,"emptyRead":empty},"erasureProvider":provider,"graphSha256":bytes_sha(&graph),"overlaySha256":sha(&overlay)?,"originalCaseSha256":READER_TEST_SHA,"observationsSha256":witness,"baseline":normal,"sourceWidthMutation":extent,"refillPolicyMutation":refill,"recovery":load(&baseline.join("recovery.lock"))?}),
        )
    }
}
fn version(tool: &str) -> Result<String> {
    Ok(stdout(Command::new("rustc").args([tool, "--version"]))?
        .trim()
        .into())
}
fn main() -> Result {
    let a = Args::parse();
    let original = a.original.canonicalize()?;
    let framework = a.framework.canonicalize()?;
    let pin = load(&whole().join("private-buffer/compiler.json"))?;
    let fw = text(&pin["revision"])?;
    checkout(&original, ORIGINAL, false)?;
    checkout(&framework, fw, false)?;
    fs::create_dir_all(&a.compiler_cache)?;
    let target = a.compiler_cache.canonicalize()?;
    let mut build = Command::new("cargo");
    build
        .current_dir(&framework)
        .args([
            "+1.96.1",
            "build",
            "--offline",
            "--locked",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(&target)
        .env("RUSTUP_TOOLCHAIN", "1.97.1")
        .env("CARGO_NET_OFFLINE", "true");
    for (k, _) in env::vars_os() {
        let key = k.to_string_lossy();
        if matches!(
            key.as_ref(),
            "RUSTC"
                | "RUSTFLAGS"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "RUSTC_BOOTSTRAP"
                | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER"
                | "CARGO_BUILD_TARGET"
        ) || key.starts_with("CARGO_PROFILE_")
        {
            build.env_remove(k);
        }
    }
    run(&mut build)?;
    let info = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = info
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .ok_or("missing compiler host")?
        .to_owned();
    let temp = tempfile::tempdir()?;
    let source = reader_source(&original)?;
    let module = original_test(&source)?;
    let suite = Suite {
        original,
        framework,
        cli: target.join("debug/oddlang"),
        root: temp.path().canonicalize()?,
        host,
        module,
    };
    let mut result = suite.verify(&source)?;
    result["framework"] = fw.into();
    result["frameworkCompiler"] = version("+1.96.1")?.into();
    result["productCompiler"] = version("+1.97.1")?.into();
    fs::write(a.receipt, json_bytes(&result)?)?;
    Ok(())
}
