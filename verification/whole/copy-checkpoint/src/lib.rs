pub mod frame_pair;
use public_data_qualification::{Result, assembly, capture, load, portable, require, run};
use serde::Serialize;
use serde_json::{Value, json, ser::Formatter};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub fn stage(work: &Path, file: &str, include_wire: bool) -> Result<(PathBuf, Value)> {
    fs::create_dir(work)?;
    let src = portable().join(file);
    let manifest = load(&src)?;
    assembly::pinned_copy(&portable(), &manifest["cargoLock"], work)?;
    for c in manifest["components"]
        .as_array()
        .ok_or("missing components")?
    {
        for key in ["graph", "overlay"] {
            let p = assembly::pinned_copy(&portable(), &c[key], work)?;
            require(
                p.extension().is_none_or(|e| e != "rs"),
                "unexpected source in projection closure",
            )?;
        }
        if include_wire && let Some(shape) = c.get("wireShape") {
            for pin in shape.as_object().ok_or("invalid wire shape")?.values() {
                assembly::pinned_copy(&portable(), pin, work)?;
            }
        }
    }
    let plan = work.join(file);
    fs::copy(src, &plan)?;
    Ok((plan, manifest))
}
pub fn build_compiler(framework: &Path, target: &Path, toolchain: &str) -> Result<PathBuf> {
    run(Command::new("cargo")
        .current_dir(framework)
        .arg(toolchain)
        .args([
            "build",
            "--locked",
            "--offline",
            "--bin",
            "oddlang",
            "--target-dir",
        ])
        .arg(target))?;
    Ok(target.join("debug/oddlang"))
}
/// The archived repinned manifest uses Python's single-line JSON with spaces after separators.
struct Spaced;
impl Formatter for Spaced {
    fn begin_array_value<W: Write + ?Sized>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if first { Ok(()) } else { w.write_all(b", ") }
    }
    fn begin_object_key<W: Write + ?Sized>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.begin_array_value(w, first)
    }
    fn begin_object_value<W: Write + ?Sized>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(b": ")
    }
}
pub fn python_line(v: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    v.serialize(&mut serde_json::Serializer::with_formatter(
        &mut bytes, Spaced,
    ))?;
    let mut out = String::new();
    for c in String::from_utf8(bytes)?.chars() {
        if c.is_ascii() {
            out.push(c)
        } else {
            use std::fmt::Write;
            for n in c.encode_utf16(&mut [0; 2]) {
                write!(out, "\\u{n:04x}")?;
            }
        }
    }
    Ok(out.into_bytes())
}
pub fn consumer(root: &Path, work: &Path, lock: &Path, caller: &Path) -> Result<PathBuf> {
    fs::create_dir_all(work.join("src"))?;
    fs::copy(caller, work.join("src/main.rs"))?;
    fs::write(
        work.join("Cargo.toml"),
        format!(
            "[package]\nname=\"observe-copy-launch\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nserde={{version=\"=1.0.229\",features=[\"derive\"]}}\nserde_json=\"=1.0.151\"\nmarmish-stream={{path={}}}\n[workspace]\n",
            json!(root)
        ),
    )?;
    fs::copy(lock, work.join("Cargo.lock"))?;
    for args in [
        &["metadata", "--offline", "--format-version", "1"][..],
        &["build", "--release", "--offline", "--locked"],
    ] {
        run(Command::new("cargo")
            .env_remove("CARGO_TARGET_DIR")
            .current_dir(work)
            .arg("+1.97.1")
            .args(args))?;
    }
    Ok(work.join("target/release/observe-copy-launch"))
}
pub fn compare(label: &str, exe: &Path, requests: &[Value], expected: &[Value]) -> Result {
    let mut input = Vec::new();
    for r in requests {
        serde_json::to_writer(&mut input, r)?;
        writeln!(input)?;
    }
    let out = capture(&mut Command::new(exe), 600, Some(&input))?;
    require(out.status.success(), String::from_utf8_lossy(&out.stderr))?;
    let lines = std::str::from_utf8(&out.stdout)?.lines();
    require(
        lines.clone().count() == expected.len(),
        format!("{label} result count differs"),
    )?;
    for (i, (line, want)) in lines.zip(expected).enumerate() {
        require(
            serde_json::from_str::<Value>(line)? == *want,
            format!("{label} case {i} differs"),
        )?;
    }
    Ok(())
}
