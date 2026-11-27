//! Exercise all four newly exposed data types against the selected native workspace libraries.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
struct Caller {
    flags: Vec<OsString>,
    dir: PathBuf,
}
impl Caller {
    fn new(
        host: &str,
        work: PathBuf,
        libraries: &[(&str, &Path)],
        search: Vec<PathBuf>,
        links: &[String],
    ) -> Result<Self> {
        fs::create_dir(&work)?;
        let mut flags: Vec<OsString> = ["+1.97.1", "--edition=2024", "--target", host]
            .map(Into::into)
            .into();
        for (name, path) in libraries {
            flags.push("--extern".into());
            flags.push(format!("{name}={}", path.display()).into());
        }
        let mut seen = BTreeSet::new();
        for p in search {
            if seen.insert(p.clone()) {
                flags.push("-L".into());
                flags.push(format!("dependency={}", p.display()).into());
            }
        }
        for p in links {
            flags.push("-L".into());
            flags.push(p.into());
        }
        Ok(Self { flags, dir: work })
    }
    fn observer(&self, script: &str) -> Result<PathBuf> {
        let binary = self.dir.join("probe");
        run(Command::new("rustc")
            .args(&self.flags)
            .arg(whole().join(script))
            .arg("-o")
            .arg(&binary))?;
        Ok(binary)
    }
    fn bound(&self, body: &str, expected: Option<&str>) -> Result<String> {
        let path = self.dir.join("check.rs");
        fs::write(&path, body)?;
        let result = capture(
            Command::new("rustc")
                .args(&self.flags)
                .args(["--emit=metadata", "--error-format=json"])
                .arg(path)
                .arg("-o")
                .arg(self.dir.join("check.rmeta")),
            60,
            None,
        )?;
        let mut codes = Vec::new();
        for line in std::str::from_utf8(&result.stderr)?.lines() {
            let r: Value = serde_json::from_str(line)?;
            if r["level"] == "error" && !r["code"].is_null() {
                codes.push(text(&r["code"]["code"])?.to_string());
            }
        }
        require(
            result.status.success() == expected.is_none()
                && codes.iter().map(String::as_str).eq(expected),
            format!("unexpected diagnostic for {body}: {codes:?}"),
        )?;
        Ok(expected.unwrap_or("available").into())
    }
}
fn fieldless(caller: &Caller) -> Result<Value> {
    let program = caller.observer("engine-fieldless-choices/probe.rs")?;
    let out = stdout(&mut Command::new(program))?;
    let mut traits = json!({});
    for name in ["Stream", "SubmitDisposition"] {
        for (kind, bound) in [
            ("Copy", "Copy"),
            ("Default", "Default"),
            ("Ord", "Ord"),
            ("PartialOrd", "PartialOrd"),
            ("Hash", "std::hash::Hash"),
            ("Display", "std::fmt::Display"),
            ("Serialize", "serde::Serialize"),
            ("Deserialize", "for<'de> serde::Deserialize<'de>"),
        ] {
            let code = if kind == "Copy" && name == "Stream" {
                None
            } else {
                Some("E0277")
            };
            let src = format!(
                "fn require<T: {bound}>() {{}} fn main() {{ require::<marmish_engine::{name}>(); }}"
            );
            traits[format!("{name}.{kind}")] = caller.bound(&src, code)?.into();
        }
    }
    traits["privateModule"] = caller
        .bound(
            "use marmish_engine::model::Stream; fn main() {}",
            Some("E0603"),
        )?
        .into();
    Ok(json!({"values":out.lines().collect::<Vec<_>>(),"traits":traits}))
}
fn runtime(caller: &Caller) -> Result<Value> {
    let program = caller.observer("runtime-status-data/probe.rs")?;
    let out = stdout(Command::new(program).current_dir(&caller.dir))?;
    let mut diagnostics = json!({});
    for (label, bound) in [
        ("clone", "Clone"),
        ("debug", "std::fmt::Debug"),
        ("default", "Default"),
        ("deserialize", "for<'de> serde::Deserialize<'de>"),
    ] {
        let src = format!(
            "use marmish_native::mdk_adapter::RuntimeStatus;\nfn need<T: {bound}>() {{}} fn main() {{ need::<RuntimeStatus>(); }}"
        );
        diagnostics[label] = caller.bound(&src, Some("E0277"))?.into();
    }
    diagnostics["wordType"]=caller.bound("use marmish_native::mdk_adapter::RuntimeStatus;\nfn main() { let _ = RuntimeStatus { identity: String::new(), conversations: 0u64, received: 0, pending: 0 }; }",Some("E0308"))?.into();
    diagnostics["rootExposure"] = caller
        .bound(
            "use marmish_native::RuntimeStatus; fn main() {}",
            Some("E0432"),
        )?
        .into();
    Ok(json!({"jsonLines":out.lines().collect::<Vec<_>>(),"diagnostics":diagnostics}))
}
fn reason(caller: &Caller) -> Result<Value> {
    let program = caller.observer("gap-reason-recovery/probe.rs")?;
    let catalog = load(&whole().join("gap-range-reference/cases.json"))?;
    let cases: Vec<_> = list(&catalog)?
        .iter()
        .filter(|r| r["type"] == "reason")
        .collect();
    require(cases.len() == 17, "unexpected MissingReason case count")?;
    let result = capture(
        Command::new(program)
            .arg("baseline")
            .current_dir(&caller.dir),
        600,
        Some(&serde_json::to_vec(&cases)?),
    )?;
    require(
        result.status.success(),
        String::from_utf8_lossy(&result.stderr),
    )?;
    let observed: Value = serde_json::from_slice(&result.stdout)?;
    let code = caller.bound(
        "fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }",
        Some("E0277"),
    )?;
    Ok(json!({"output":observed,"copyControl":[code]}))
}
pub fn observe(input: &Value) -> Result<Value> {
    let host = text(&input["host"])?;
    let work = Path::new(text(&input["work"])?);
    let target = Path::new(text(&input["target"])?);
    let roles = &input["roles"];
    let engine = Path::new(text(&roles["marmish-engine"][1])?);
    let native = Path::new(text(&roles["marmish-native"][1])?);
    let serde = Path::new(text(&input["serde"])?);
    let serde_json = Path::new(text(&input["serdeJson"])?);
    let links: Vec<String> = serde_json::from_value(input["links"].clone())?;
    let hostdeps = target.join("release/deps");
    let targetdeps = target.join(host).join("release/deps");
    let engine_parent = engine.parent().ok_or("invalid engine library")?;
    let choices = fieldless(&Caller::new(
        host,
        work.join("engine-fieldless"),
        &[("marmish_engine", engine), ("serde", serde)],
        vec![
            engine_parent.into(),
            engine_parent.join("deps"),
            serde.parent().ok_or("invalid serde library")?.into(),
            hostdeps.clone(),
        ],
        &links,
    )?)?;
    let runtime = runtime(&Caller::new(
        host,
        work.join("native-status"),
        &[
            ("marmish_native", native),
            ("serde", serde),
            ("serde_json", serde_json),
        ],
        vec![targetdeps.clone(), hostdeps.clone()],
        &links,
    )?)?;
    let reason = reason(&Caller::new(
        host,
        work.join("missing-reason"),
        &[("marmish_engine", engine), ("serde_json", serde_json)],
        vec![targetdeps, hostdeps],
        &links,
    )?)?;
    let receipt = |dir: &str| whole().join(dir).join(format!("{host}.json"));
    let fieldless_file = receipt("engine-fieldless-choices");
    let fieldless = load(&fieldless_file)?;
    let fieldless = &fieldless["scenarios"]["baseline"];
    require(
        choices == json!({"values":fieldless["values"],"traits":fieldless["traits"]}),
        "combined engine choice caller differs from pinned standalone witness",
    )?;
    let runtime_file = receipt("runtime-status-data");
    require(
        runtime == load(&runtime_file)?["observations"],
        "combined RuntimeStatus differs from pinned external caller",
    )?;
    let reason_file = receipt("gap-reason-recovery");
    let prior = load(&reason_file)?;
    let prior = &prior["lanes"]["baseline"];
    require(
        reason["output"] == prior["output"] && reason["copyControl"] == prior["copyControl"],
        "combined MissingReason differs from pinned native wire and type witnesses",
    )?;
    Ok(json!({
        "fieldlessChoices": choices,
        "runtimeStatus": runtime,
        "missingReason": reason,
        "candidateEngineLibrarySha256": sha(engine)?,
        "candidateNativeLibrarySha256": sha(native)?,
        "originalWitnessReceiptSha256": {"fieldlessChoices":sha(&fieldless_file)?,
        "runtimeStatus": sha(&runtime_file)?,
        "missingReason": sha(&reason_file)?},
    }))
}
