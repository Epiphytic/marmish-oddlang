//! Exercise the same retained upstream test and independent caller on both Rust libraries.
use core_qualification::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const TEST: &str = "retention_distinguishes_default_from_disabled_and_bounds_numbers";
const COMPILER: &str = "8bab26f4f68e0e26f0bb7960be334d5b520ea452";
fn digest(data: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn rustc() -> Command {
    let mut c = Command::new("rustc");
    c.arg("+1.97.1");
    c
}
fn generate(
    cli: &Path,
    work: &Path,
    name: &str,
    domain: &str,
    overlay: &[u8],
    supplied: Option<&Path>,
) -> Result<(PathBuf, Value)> {
    let source = work.join(format!("{name}-graph"));
    fs::create_dir(&source)?;
    fs::write(source.join("domain.ttl"), domain)?;
    let binding = source.join("rust.json");
    fs::write(&binding, overlay)?;
    let dest = work.join(format!("{name}-release"));
    output(
        Command::new(cli)
            .args(["build", "value-domain", "rust"])
            .arg(&source)
            .arg(&binding)
            .arg(&dest)
            .current_dir(work),
    )?;
    let lock = load(&dest.join("projection.lock"))?;
    for k in ["domainSha256", "bindingSha256"] {
        let s = txt(&lock[k])?;
        require(
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid projection input digest",
        )?;
    }
    require(
        lock == json!({
            "format": "oddlang-value-domain-v1",
            "domainSha256": lock["domainSha256"],
            "bindingSha256": lock["bindingSha256"],
            "artifact": "value.rs",
            "artifactSha256": sha(&dest.join("value.rs"))?,
            "obligationsSha256": sha(&dest.join("obligations.json"))?
        }),
        "projection lock differs from actual graph and generated artifacts",
    )?;
    if let Some(dir) = supplied {
        for file in ["value.rs", "obligations.json", "projection.lock"] {
            require(
                fs::read(dir.join(file))? == fs::read(dest.join(file))?,
                format!("fresh graph-only generation differs from supplied {file}"),
            )?;
        }
    }
    let lib = dest.join("libmarmish_core.rlib");
    output(
        rustc()
            .args([
                "--edition=2021",
                "--crate-type=rlib",
                "--crate-name=marmish_core",
            ])
            .arg(dest.join("value.rs"))
            .arg("-o")
            .arg(&lib)
            .current_dir(work),
    )?;
    Ok((lib, lock))
}
fn compile_caller(
    work: &Path,
    name: &str,
    library: &Path,
    deps: Option<PathBuf>,
) -> Result<PathBuf> {
    let binary = work.join(name);
    let mut cmd = rustc();
    cmd.current_dir(work)
        .args(["--edition=2021", "--test"])
        .arg(work.join("caller.rs"))
        .arg("--extern")
        .arg(format!("marmish_core={}", library.display()));
    if let Some(dir) = deps {
        cmd.arg("-L").arg(format!("dependency={}", dir.display()));
    }
    output(cmd.arg("-o").arg(&binary))?;
    Ok(binary)
}
fn run_caller(work: &Path, binary: &Path) -> Result<Vec<String>> {
    let text = output(
        Command::new(binary)
            .args(["--show-output", "--test-threads=1"])
            .current_dir(work),
    )?;
    require(
        text.contains("5 passed; 0 failed"),
        "did not execute the full caller",
    )?;
    let rows: Vec<_> = text
        .lines()
        .filter(|s| s.starts_with("OBS\t"))
        .map(str::to_owned)
        .collect();
    require(
        rows.len() == 790,
        format!("caller emitted {} observations", rows.len()),
    )?;
    Ok(rows)
}
fn original_library(original: &Path, target: &Path) -> Result<PathBuf> {
    let cargo = |sub: &str| {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(original)
            .args([
                "+1.97.1",
                sub,
                "--locked",
                "--offline",
                "--release",
                "-p",
                "marmish-core",
                "--target-dir",
            ])
            .arg(target);
        cmd
    };
    let output_text = output(cargo("build").arg("--message-format=json"))?;
    let mut libraries = vec![];
    for line in output_text.lines() {
        let item: Value = serde_json::from_str(line)?;
        if item["reason"] == "compiler-artifact" && item["target"]["name"] == "marmish_core" {
            for file in arr(&item["filenames"])? {
                let name = txt(file)?;
                if name.ends_with(".rlib") {
                    libraries.push(PathBuf::from(name));
                }
            }
        }
    }
    let library = one(libraries.into_iter())?;
    let test = output(cargo("test").args(["--test", "contracts", "--", TEST, "--exact"]))?;
    require(
        test.contains(&format!("test {TEST} ... ok")),
        "the unchanged original integration test did not execute",
    )?;
    Ok(library)
}
fn unchanged_function(src: &str) -> Result<&str> {
    let header = format!("#[test]\nfn {TEST}() {{\n");
    let start = src
        .match_indices(&header)
        .find(|(i, _)| *i == 0 || src.as_bytes()[i - 1] == b'\n')
        .ok_or("original retention test function missing")?
        .0;
    let body = &src[start..];
    let end = body
        .find("\n}\n")
        .ok_or("missing original retention test terminator")?
        + 3;
    Ok(&body[..end])
}
fn main() -> Result {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [original, supplied, cli, receipt] = args.as_slice() else {
        return Err("usage: core-value ORIGINAL GENERATED ODDLANG RECEIPT".into());
    };
    let original = original.canonicalize()?;
    let supplied = supplied.canonicalize()?;
    let cli = cli.canonicalize()?;
    let compiler = output(rustc().arg("-vV"))?;
    require(
        compiler
            .lines()
            .any(|l| l == format!("commit-hash: {COMPILER}")),
        "unexpected Rust compiler",
    )?;
    pinned_git(&original, ORIGINAL)?;
    let test = original.join("crates/marmish-core/tests/contracts.rs");
    let contents = fs::read_to_string(&test)?;
    let unchanged = unchanged_function(&contents)?;
    let contract = portable().join("core-config-v1");
    let domain = fs::read_to_string(contract.join("domain.ttl"))?;
    let overlay = fs::read(contract.join("rust.json"))?;
    let old_limit = "c:maximum 31536000";
    require(
        domain.matches(old_limit).count() == 1,
        "upper bound mutation must alter exactly one triple",
    )?;
    let mutant = domain.replacen(old_limit, "c:maximum 31535999", 1);
    let temp = tempfile::tempdir()?;
    let work = temp.path();
    let (generated, pins) = generate(&cli, work, "generated", &domain, &overlay, Some(&supplied))?;
    require(
        load(&contract.join("rust.lock"))? == pins,
        "committed Rust projection lock mismatch",
    )?;
    let ts = work.join("typescript");
    output(
        Command::new(&cli)
            .args(["build", "value-domain", "typescript"])
            .arg(&contract)
            .arg(&ts),
    )?;
    require(
        fs::read(ts.join("projection.lock"))? == fs::read(contract.join("typescript.lock"))?,
        "committed TypeScript projection lock mismatch",
    )?;
    let node: Value = serde_json::from_str(&output(
        Command::new("node")
            .arg("--experimental-strip-types")
            .arg(whole().join("core-node/check.mjs"))
            .arg(ts),
    )?)?;
    let observed = work.join("observation");
    output(
        Command::new(&cli)
            .args(["recover", "value-domain"])
            .arg(original.join("Cargo.toml"))
            .arg(contract.join("selection.json"))
            .arg(&observed),
    )?;
    for file in ["domain.ttl", "observation.json"] {
        require(
            fs::read(observed.join(file))? == fs::read(contract.join(file))?,
            format!("source recovery differs from committed {file}"),
        )?;
    }
    let (changed, changed_pins) = generate(&cli, work, "mutant", &mutant, &overlay, None)?;
    require(
        changed_pins["domainSha256"] != pins["domainSha256"]
            && changed_pins["bindingSha256"] == pins["bindingSha256"],
        "mutation must alter the projected domain but not its Rust binding",
    )?;
    let library = original_library(&original, &work.join("upstream"))?;
    let caller = whole().join("core-value/caller.rs");
    let mut harness = fs::read(&caller)?;
    harness.push(b'\n');
    harness.extend(unchanged.as_bytes());
    fs::write(work.join("caller.rs"), harness)?;
    let old = compile_caller(
        work,
        "original",
        &library,
        Some(library.with_file_name("deps")),
    )?;
    let new = compile_caller(work, "generated", &generated, None)?;
    let changed = compile_caller(work, "mutant", &changed, None)?;
    let old = run_caller(work, &old)?;
    let new = run_caller(work, &new)?;
    require(old == new, "original and generated observations differ")?;
    let failure = Command::new(changed)
        .args(["independent_boundaries", "--exact", "--test-threads=1"])
        .current_dir(work)
        .output()?;
    let control = "rejected allowed input Some(31536000)";
    require(
        failure.status.code() == Some(101)
            && std::str::from_utf8(&failure.stdout)?.contains(control),
        "boundary oracle rejected mutation for an unexpected reason",
    )?;
    let observation = |rows: &[String]| json!({"count":rows.len(),"sha256":digest(format!("{}\n",rows.join("\n")))});
    let tree = output(
        Command::new("git")
            .current_dir(&original)
            .args(["rev-parse", "HEAD^{tree}"]),
    )?;
    save(
        receipt,
        &json!({
            "original": {
                "revision": ORIGINAL,
                "tree": tree.trim(),
                "cargoLockSha256": sha(&original.join("Cargo.lock"))?,
                "contractsFileSha256": sha(&test)?,
                "retentionTestBodySha256": digest(unchanged),
                "retentionTest": TEST,
                "retentionTestResult": "passed"
            },
            "generated": {
                "projectionLock": pins,
                "domainFileSha256": digest(domain),
                "rustBindingFileSha256": digest(overlay),
                "regeneratedFromGraphAndBinding": true
            },
            "typescript": node,
            "rustc": compiler.lines().collect::<Vec<_>>(),
            "callerSha256": sha(&caller)?,
            "cases": {
                "boundaryInputs": 14,
                "seededInputs": 768,
                "errorMessages": 7,
                "publicSurface": 1,
                "unchangedTestFunctionOnBoth": true
            },
            "observations": {"original": observation(&old), "generated": observation(&new)},
            "mutation": {
                "graphSha256": digest(mutant),
                "projectionLock": changed_pins,
                "maximum": 31535999,
                "control": control,
                "result": "rejected"
            }
        }),
    )
}
