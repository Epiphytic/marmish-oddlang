//! Select Cargo artifacts and verify both native parser roles.
use crate::{
    assembly::{ORIGINAL, build_env, bytes_sha},
    manifest_product::{self as parser, *},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub fn artifacts(rows: &[Value], name: &str, ext: &str) -> Result<BTreeSet<PathBuf>> {
    let mut out = BTreeSet::new();
    for row in rows
        .iter()
        .filter(|r| r["reason"] == "compiler-artifact" && r["target"]["name"] == name)
    {
        for f in list(&row["filenames"])? {
            let p = PathBuf::from(text(f)?);
            if p.extension().is_some_and(|s| s == ext) {
                out.insert(p);
            }
        }
    }
    Ok(out)
}
pub fn one(rows: &[Value], name: &str) -> Result<PathBuf> {
    let s = artifacts(rows, name, "rlib")?;
    require(s.len() == 1, format!("ambiguous {name} library"))?;
    Ok(s.into_iter().next().unwrap())
}
pub fn rows(raw: &[u8]) -> Result<Vec<Value>> {
    Ok(std::str::from_utf8(raw)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?)
}
pub fn reference(root: &Path, target: &Path, name: &str) -> Result<Vec<Value>> {
    rows(&run(build_env(
        Command::new("cargo")
            .current_dir(root)
            .args([
                "+1.97.1",
                "build",
                "--release",
                "--locked",
                "--offline",
                "--lib",
                "-p",
                name,
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target),
    ))?)
}
pub fn diagnostics(raw: &[u8]) -> Result<Vec<Value>> {
    let mut errors = Vec::new();
    for line in std::str::from_utf8(raw)?
        .lines()
        .filter(|l| l.starts_with('{'))
    {
        let r: Value = serde_json::from_str(line)?;
        if r["level"] == "error" && !r["code"].is_null() {
            errors.push(r);
        }
    }
    Ok(errors)
}
pub fn depend(cmd: &mut Command, dir: &Path) {
    cmd.arg("-L").arg(format!("dependency={}", dir.display()));
}
pub fn external(cmd: &mut Command, name: &str, path: &Path) {
    cmd.arg("--extern")
        .arg(format!("{name}={}", path.display()));
}
pub fn parent(path: &Path) -> Result<&Path> {
    Ok(path.parent().ok_or("missing artifact directory")?)
}
fn privacy(build: &[Value], source: &Path, output: &Path, host: &str) -> Result<Value> {
    let metadata = artifacts(build, "marmish_native", "rmeta")?;
    require(metadata.len() <= 1, "ambiguous native metadata")?;
    let library = match metadata.into_iter().next() {
        Some(path) => path,
        None => one(build, "marmish_native")?,
    };
    let serde = one(build, "serde_json")?;
    let mut c = Command::new("rustc");
    c.args([
        "+1.97.1",
        "--edition=2024",
        "--target",
        host,
        "--crate-type=lib",
        "--emit=metadata",
        "--error-format=json",
    ])
    .arg(source);
    depend(&mut c, parent(&serde)?);
    depend(&mut c, parent(&library)?);
    external(&mut c, "marmish_native", &library);
    c.arg("-o").arg(output);
    let mut links = BTreeSet::new();
    for row in build
        .iter()
        .filter(|r| r["reason"] == "build-script-executed")
    {
        for l in list(&row["linked_paths"])? {
            links.insert(text(l)?);
        }
    }
    for l in links {
        c.arg("-L").arg(l);
    }
    let output = capture(&mut c, 60, None)?;
    let errors = diagnostics(&output.stderr)?;
    require(
        !output.status.success()
            && errors.len() == 1
            && errors[0]["code"]["code"] == "E0603"
            && text(&errors[0]["message"])?.contains("module `native_manifest` is private"),
        format!(
            "wrong private parser diagnostic: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok("E0603".into())
}
fn tool(package: &str, binary: &str, target: &Path) -> Command {
    let mut c = Command::new("cargo");
    let ver = if package == "release-manifest-projection" {
        "+1.97.1"
    } else {
        "+1.96.1"
    };
    c.args([
        ver,
        "run",
        "--quiet",
        "--locked",
        "--offline",
        "--manifest-path",
    ])
    .arg(whole().join(package).join("Cargo.toml"))
    .arg("--target-dir")
    .arg(target)
    .args(["--bin", binary, "--"]);
    c
}
/// Encode the case and byte-pair stream exactly as received by the pinned upstream test.
pub fn write_native_inputs(generated: &Path, path: &Path) -> Result {
    let doc = load(generated)?;
    let mut buffer = String::from("[");
    for (i, v) in doc
        .as_array()
        .ok_or("expected input rows")?
        .iter()
        .enumerate()
    {
        // The original native test received these ordered objects as one ASCII line.
        if i > 0 {
            buffer.push_str(", ");
        }
        buffer.push_str(&format!(
            "{{\"case\": {}, \"hex\": {}}}",
            serde_json::to_string(text(&v["case"])?)?,
            serde_json::to_string(text(&v["hex"])?)?
        ));
    }
    buffer.push(']');
    fs::write(path, buffer)?;
    Ok(())
}
pub struct ParserRun<'a> {
    pub product: &'a Path,
    pub original: &'a Path,
    pub original_target: &'a Path,
    pub scratch: &'a Path,
    pub host: &'a str,
    pub build: &'a [Value],
}
impl ParserRun<'_> {
    pub fn observe(&self, raw_results: Option<&Path>) -> Result<Value> {
        let ParserRun {
            product,
            original,
            original_target,
            scratch,
            host,
            build,
        } = *self;
        let native = product.join(MEMBER);
        let (leaf, prefix) = parser::leaf(&native)?;
        let source = scratch.join("outside.rs");
        fs::write(
            &source,
            "pub use marmish_native::native_manifest::Manifest;\n",
        )?;
        let original_build = reference(original, original_target, MEMBER)?;
        let private = json!({"original":privacy(&original_build,&source,&scratch.join("original.rmeta"),host)?,
                           "generated":privacy(build,&source,&scratch.join("generated.rmeta"),host)?});
        let tools = scratch.join("manifest-tools");
        let shape: Value = serde_json::from_slice(&run(tool(
            "release-manifest-projection",
            "compare-records",
            &tools,
        )
        .arg(original)
        .arg(original_target)
        .arg(product)
        .arg(product.join("target"))
        .arg(leaf.strip_prefix(product)?)
        .arg(portable().join(GRAPH))
        .arg(portable().join(BINDING)))?)?;
        let tests = parser::tests(original)?;
        let vectors = original.join(VECTOR);
        let archive = whole()
            .join("release-manifest")
            .join(format!("{host}.json"));
        let official = load(&archive)?;
        let passed: Vec<_> = CASES
            .iter()
            .map(|c| format!("test native_manifest::tests::{c} ... ok"))
            .collect();
        require(
            official["originalRevision"] == ORIGINAL
                && official["sourcePins"][SOURCE] == sha(&original.join(SOURCE))?
                && official["sourcePins"][VECTOR] == sha(&vectors)?
                && official["unchangedPureTests"] == json!(passed),
            "original parser witness or tests changed",
        )?;
        let generated = scratch.join("corpus-inputs.json");
        let blind = scratch.join("blind-manifest-inputs.json");
        run(
            tool("release-manifest", "release-manifest-conformance", &tools)
                .arg("inputs")
                .arg(&generated),
        )?;
        write_native_inputs(&generated, &blind)?;
        let caller = whole().join("release-manifest/probe.rs");
        require(
            official["inputsSha256"] == sha(&blind)? && official["callerSha256"] == sha(&caller)?,
            "native parser inputs or caller differ from pinned upstream observation",
        )?;
        let result = scratch.join("integrated.json");
        let tmp = buffered::test_workspace(product)?;
        let staged = tmp.path();
        let native = staged.join(MEMBER);
        let staged_leaf = staged.join(leaf.strip_prefix(product)?);
        let lib = native.join("src/lib.rs");
        let probe = native.join("src/manifest_oracle.rs");
        let target = staged.join(VECTOR);
        require(
            !product.join(VECTOR).try_exists()?
                && !product
                    .join(MEMBER)
                    .join("src/manifest_oracle.rs")
                    .try_exists()?,
            "original test inputs already present in generated package",
        )?;
        fs::create_dir_all(parent(&target)?)?;
        fs::copy(&vectors, target)?;
        fs::copy(&caller, &probe)?;
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(staged_leaf)?;
        f.write_all(b"\n")?;
        f.write_all(&tests)?;
        drop(f);
        fs::OpenOptions::new().append(true).open(lib)?.write_all(
            b"\n#[cfg(test)]\n#[path = \"manifest_oracle.rs\"]\nmod manifest_oracle;\n",
        )?;
        let names: Vec<_> = CASES
            .iter()
            .map(|c| format!("{prefix}::tests::{c}"))
            .collect();
        for name in names
            .iter()
            .map(String::as_str)
            .chain(["manifest_oracle::manifest_contract_probe"])
        {
            let command = Command::new("cargo");
            let mut c = command;
            build_env(c.current_dir(staged))
                .env("MANIFEST_CASES", &blind)
                .env("MANIFEST_RESULT", &result)
                .args([
                    "+1.97.1",
                    "test",
                    "--release",
                    "--locked",
                    "--offline",
                    "--target-dir",
                ])
                .arg(scratch.join("manifest-test-target"))
                .args(["-p", MEMBER, "--lib", "--", "--exact", name]);
            let o = capture(&mut c, 600, None)?;
            let stdout = String::from_utf8(o.stdout)?;
            require(
                o.status.success()
                    && stdout
                        .lines()
                        .filter(|l| *l == format!("test {name} ... ok"))
                        .count()
                        == 1,
                format!(
                    "missing native parser case {name}: {stdout}{}",
                    String::from_utf8_lossy(&o.stderr)
                ),
            )?;
        }
        run(
            tool("release-manifest", "release-manifest-conformance", &tools)
                .arg("check")
                .arg(&result),
        )?;
        let data = load(&result)?;
        let rows = list(&data)?;
        require(
            data == official["baseline"],
            "integrated parser differs from the upstream result rows",
        )?;
        let standalone = whole()
            .join("release-manifest-projection")
            .join(format!("{host}.json"));
        let hash = sha(&result)?;
        require(
            load(&standalone)?["lanes"]["baseline"]["resultsSha256"] == hash,
            "integrated result bytes differ from standalone release",
        )?;
        if let Some(path) = raw_results {
            fs::copy(&result, path)?;
        }
        Ok(
            json!({"scope":"crate-private Manifest::parse, id and asset_name","cases":rows.len(),"accepted":rows.iter().filter(|r|r["result"].get("error").is_none()).count(),
                  "resultsSha256":hash,"originalOracleSha256":sha(&archive)?,"standaloneReceiptSha256":sha(&standalone)?,
                  "nativeRecords":shape,"privateDownstreamImport":private,"unchangedTestModuleSha256":bytes_sha(&tests),
                  "originalVectorIncludeSha256":sha(&vectors)?,"unchangedPureTests":names}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_witness_receives_identical_blind_bytes() -> Result {
        let lane = whole().join("release-manifest");
        let archive = load(&lane.join("aarch64-apple-darwin.json"))?;
        let temp = tempfile::tempdir()?;
        let cases = temp.path().join("inputs.json");
        write_native_inputs(&lane.join("vectors.json"), &cases)?;
        assert_eq!(archive["inputsSha256"], sha(&cases)?);
        assert_eq!(archive["callerSha256"], sha(&lane.join("probe.rs"))?);
        let selected = load(&cases)?;
        let input_rows = list(&selected)?;
        assert_eq!(input_rows.len(), 224);
        assert!(
            input_rows
                .iter()
                .all(|row| row.as_object().is_some_and(|obj| obj.len() == 2
                    && obj.contains_key("case")
                    && obj.contains_key("hex")))
        );
        Ok(())
    }
}
