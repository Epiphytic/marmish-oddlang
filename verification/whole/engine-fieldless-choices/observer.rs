//! Compile downstream callers against the selected engine and serde libraries.
use clap::Args;
use public_data_qualification::{Result, capture, require, run, whole};
use serde_json::{Value, json};
use std::{ffi::OsString, fs, path::PathBuf, process::Command};

#[derive(Args)]
pub struct Observer {
    pub library: PathBuf,
    pub serde: PathBuf,
    pub host_deps: PathBuf,
    pub host: String,
    pub work: PathBuf,
    #[arg(long)]
    pub copy_disposition: bool,
    #[arg(long)]
    pub link: Vec<OsString>,
}

impl Observer {
    fn compiler(&self) -> Result<Command> {
        let mut cmd = Command::new("rustc");
        cmd.args(["+1.97.1", "--target", &self.host, "--edition=2024"]);
        for (name, lib) in [("marmish_engine=", &self.library), ("serde=", &self.serde)] {
            let mut value = OsString::from(name);
            value.push(lib);
            cmd.arg("--extern").arg(value);
        }
        let lib_dir = self.library.parent().ok_or("missing engine directory")?;
        let serde_dir = self.serde.parent().ok_or("missing serde directory")?;
        let nested = lib_dir.join("deps");
        let mut seen = std::collections::HashSet::new();
        for path in [lib_dir, &nested, serde_dir, &self.host_deps] {
            if seen.insert(path) {
                let mut value = OsString::from("dependency=");
                value.push(path);
                cmd.arg("-L").arg(value);
            }
        }
        for link in &self.link {
            cmd.arg("-L").arg(link);
        }
        Ok(cmd)
    }

    fn check(&self, source: &str, expected: Option<&str>, label: &str) -> Result<Value> {
        let output = capture(
            self.compiler()?
                .args(["--emit=metadata", "--error-format=json", "-", "-o"])
                .arg(self.work.join("check.rmeta")),
            60,
            Some(source.as_bytes()),
        )?;
        let mut codes = Vec::new();
        for line in std::str::from_utf8(&output.stderr)?.lines() {
            let row: Value = serde_json::from_str(line)?;
            if row["level"] == "error"
                && let Some(code) = row["code"]["code"].as_str()
            {
                codes.push(code.to_owned());
            }
        }
        require(
            output.status.success() == expected.is_none()
                && codes.iter().map(String::as_str).eq(expected),
            format!(
                "{label}: unexpected status {} and diagnostics {codes:?}",
                output.status
            ),
        )?;
        Ok(json!(expected.unwrap_or("available")))
    }

    pub fn observe(&self) -> Result<Value> {
        fs::create_dir(&self.work)?;
        let binary = self.work.join("probe");
        run(self
            .compiler()?
            .arg(whole().join("engine-fieldless-choices/probe.rs"))
            .arg("-o")
            .arg(&binary))?;
        let output = String::from_utf8(run(&mut Command::new(binary))?)?;
        let mut traits = serde_json::Map::new();
        for kind in ["Stream", "SubmitDisposition"] {
            for (name, bound) in [
                ("Copy", "Copy"),
                ("Default", "Default"),
                ("Ord", "Ord"),
                ("PartialOrd", "PartialOrd"),
                ("Hash", "std::hash::Hash"),
                ("Display", "std::fmt::Display"),
                ("Serialize", "serde::Serialize"),
                ("Deserialize", "for<'de> serde::Deserialize<'de>"),
            ] {
                let label = format!("{kind}.{name}");
                let source = format!(
                    "fn require<T: {bound}>() {{}} fn main() {{ require::<marmish_engine::{kind}>(); }}"
                );
                let available = name == "Copy" && (kind == "Stream" || self.copy_disposition);
                let result = self.check(&source, (!available).then_some("E0277"), &label)?;
                traits.insert(label, result);
            }
        }
        traits.insert(
            "privateModule".into(),
            self.check(
                "use marmish_engine::model::Stream; fn main() {}",
                Some("E0603"),
                "privateModule",
            )?,
        );
        Ok(json!({"values": output.lines().collect::<Vec<_>>(), "traits": traits}))
    }
}
