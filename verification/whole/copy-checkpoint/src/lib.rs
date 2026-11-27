use public_data_qualification::{Result, assembly, load, portable, require, run};
use serde::Serialize;
use serde_json::{Value, ser::Formatter};
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
