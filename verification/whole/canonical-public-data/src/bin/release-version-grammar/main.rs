//! Recover the pinned release-version grammar and exercise the native and generated predicates.
mod witness;
use clap::Parser;
use public_data_qualification::{Result, assembly::bytes_sha, load, require, run, sha, whole};
use serde_json::{Value, json};
use std::{
    env,
    ffi::OsStr,
    fs,
    ops::Range,
    path::{Path, PathBuf},
    process::Command,
};
const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
const FRAMEWORK: &str = "d322eca7fdfa2e5e257512ea477a09bb2022e9fa";
const HOST: &str = "aarch64-apple-darwin";
const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
const VECTORS: &str = "tools/release-version-vectors.json";
const TESTS: [&str; 2] = [
    "native_manifest::tests::strict_vectors",
    "native_manifest::tests::shared_release_version_vectors",
];
#[derive(Parser)]
struct Args {
    original: PathBuf,
    framework: PathBuf,
    compiler_target: PathBuf,
    reference_target: PathBuf,
    receipt: PathBuf,
}
fn command(work: &Path, policy: Option<&str>, program: impl AsRef<OsStr>) -> Command {
    let mut cmd = if let Some(p) = policy {
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.args(["-p", p, "/usr/bin/env"])
            .arg(format!("TMPDIR={}", work.display()))
            .arg(program);
        cmd
    } else {
        Command::new(program)
    };
    cmd.current_dir(work);
    cmd
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(run(Command::new("git")
        .current_dir(root)
        .args(args))?)?)
}
fn policy(work: &Path, roots: &[&Path], probes: &[PathBuf]) -> Result<String> {
    let mut policy = String::from("(version 1)(allow default)(deny network*)");
    for root in roots {
        require(
            !work.starts_with(root),
            "sandbox working root would be denied",
        )?;
        policy += &format!(
            "(deny file-read* (subpath {}))",
            serde_json::to_string(root)?
        );
    }
    for file in probes {
        require(file.is_file(), "missing denial probe")?;
        let out = command(work, Some(&policy), "/bin/cat")
            .arg(file)
            .output()?;
        require(
            !out.status.success()
                && String::from_utf8_lossy(&out.stderr).contains("Operation not permitted"),
            "denial probe could read source",
        )?;
    }
    Ok(policy)
}
fn span_json(span: &Range<usize>) -> Value {
    json!({"start":span.start,"end":span.end})
}
fn mutation(source: &str, span: &Range<usize>) -> Result<(String, Range<usize>)> {
    let mut selected = source[span.clone()].to_owned();
    let helper = selected[selected
        .find("fn decimal(")
        .ok_or("missing decimal helper")?..]
        .to_owned();
    for (old, new) in [
        ("strip_prefix(\"0.3.\")", "strip_prefix(\"0.4.\")"),
        ("split_once(\"-rc.\")", "split_once(\"-preview.\")"),
        ("Option<u64>", "Option<u32>"),
        ("parse::<u64>()", "parse::<u32>()"),
        ("decimal(rc, true)", "decimal(rc, false)"),
    ] {
        require(
            selected.matches(old).count() == 1,
            format!("ambiguous control {old}"),
        )?;
        selected = selected.replace(old, new);
    }
    require(
        selected.matches("decimal(").count() == 4,
        "unexpected helper use",
    )?;
    selected = selected.replace("decimal(", "version_decimal_control(");
    Ok((
        format!(
            "{}{selected}{helper}{}",
            &source[..span.start],
            &source[span.end..]
        ),
        span.start..span.start + selected.len(),
    ))
}
fn recover(work: &Path, cli: &Path, tag: &str, source: &str, selection: &[u8]) -> Result<PathBuf> {
    let input = work.join(format!("private/{tag}.rs"));
    let descriptor = input.with_extension("json");
    fs::write(&input, source)?;
    fs::write(&descriptor, selection)?;
    let output = work.join(format!("recovered-{tag}"));
    run(command(work, None, cli)
        .args(["recover", "text-grammar", "rust"])
        .arg(input)
        .arg(descriptor)
        .arg(&output))?;
    Ok(output)
}
fn check_shape(graph: &Value, prefix: &str, bits: u8, delimiter: &str, minimum: u8) -> Result {
    let decimal =
        |minimum: u8| json!({"notation":"canonical-ascii-decimal","bits":bits,"minimum":minimum});
    require(
        *graph
            == json!({"format":"oddlang-text-grammar-v1","id":"urn:marmish:native:release-version","prefixes":["0.2.",prefix],"body":decimal(0),"suffix":{"delimiter":delimiter,"body":decimal(minimum)},"consumption":"entire-input"}),
        "recovered grammar changed",
    )
}
fn files(dir: &Path) -> Result<std::collections::BTreeMap<std::ffi::OsString, Vec<u8>>> {
    let mut all = std::collections::BTreeMap::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        all.insert(e.file_name(), fs::read(e.path())?);
    }
    Ok(all)
}
fn main() -> Result {
    require(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "requires ARM64 macOS",
    )?;
    let args = Args::parse();
    let original = args.original.canonicalize()?;
    let framework = args.framework.canonicalize()?;
    let companion = whole().ancestors().nth(2).unwrap().canonicalize()?;
    for (root, pin) in [(&original, ORIGINAL), (&framework, FRAMEWORK)] {
        require(
            git(root, &["rev-parse", "HEAD"])?.trim() == pin
                && git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
            "unexpected source or compiler checkout",
        )?;
    }
    require(
        git(
            &companion,
            &[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--",
                "oddlang/portable",
                "verification/whole",
            ],
        )?
        .is_empty(),
        "dirty companion inputs",
    )?;
    let compiler_target = std::path::absolute(args.compiler_target)?;
    let reference_target = std::path::absolute(args.reference_target)?;
    run(Command::new("cargo")
        .current_dir(&framework)
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
    for test in TESTS {
        witness::cargo_test(&original, &reference_target, test)?;
    }
    let parent = String::from_utf8(run(
        Command::new("/usr/bin/getconf").arg("DARWIN_USER_TEMP_DIR")
    )?)?;
    let scratch = tempfile::Builder::new()
        .prefix("odd-release-version-grammar-")
        .tempdir_in(parent.trim())?;
    let work = scratch.path().canonicalize()?;
    let private = work.join("private");
    fs::create_dir(&private)?;
    let cli = work.join("oddlang");
    fs::copy(compiler_target.join("debug/oddlang"), &cli)?;
    let data = companion.join("oddlang/portable/release-version-grammar-v1");
    let source = git(&original, &["show", &format!("{ORIGINAL}:{SOURCE}")])?;
    let start = source
        .find("pub(crate) fn version(")
        .ok_or("missing version predicate")?;
    let end = start
        + source[start..]
            .find("pub(crate) fn hex32(")
            .ok_or("missing end marker")?;
    let span = start..end;
    require(
        load(&data.join("origin.json"))?
            == json!({"originalCommit":ORIGINAL,"sourcePath":SOURCE,"sourceModuleSha256":bytes_sha(source.as_bytes()),"sourceSpan":span_json(&span),"selectedFunctionsSha256":bytes_sha(source[span.clone()].as_bytes())}),
        "source origin differs",
    )?;
    let reference = private.join("native-scope");
    fs::create_dir(&reference)?;
    let tar = private.join("original.tar");
    run(Command::new("git")
        .current_dir(&original)
        .args(["archive", "--output"])
        .arg(&tar)
        .arg(ORIGINAL))?;
    run(Command::new("tar")
        .current_dir(&reference)
        .arg("-xf")
        .arg(tar))?;
    require(
        fs::read(reference.join(SOURCE))? == source.as_bytes(),
        "archive differs from Git object",
    )?;
    let native_vectors = git(&original, &["show", &format!("{ORIGINAL}:{VECTORS}")])?;
    let vectors: Value = serde_json::from_str(&native_vectors)?;
    let selection = fs::read(data.join("selection.json"))?;
    let baseline = recover(&work, &cli, "original", &source[span.clone()], &selection)?;
    for file in ["graph.json", "recovery.lock"] {
        require(
            fs::read(baseline.join(file))? == fs::read(data.join(file))?,
            "recovery artifact differs",
        )?;
    }
    check_shape(&load(&baseline.join("graph.json"))?, "0.3.", 64, "-rc.", 1)?;
    let (mutated, changed_span) = mutation(&source, &span)?;
    let mut changed_selection: Value = serde_json::from_slice(&selection)?;
    changed_selection["sourceSha256"] = json!(bytes_sha(mutated[changed_span.clone()].as_bytes()));
    let changed = recover(
        &work,
        &cli,
        "mutated",
        &mutated[changed_span.clone()],
        &serde_json::to_vec(&changed_selection)?,
    )?;
    check_shape(
        &load(&changed.join("graph.json"))?,
        "0.4.",
        32,
        "-preview.",
        0,
    )?;
    let rules = policy(
        &work,
        &[
            &original,
            &framework,
            &companion,
            &private,
            Path::new("/private/tmp"),
            &PathBuf::from(env::var_os("HOME").ok_or("missing HOME")?).join("repos"),
        ],
        &[original.join(SOURCE), private.join("mutated.rs")],
    )?;
    let boundaries: Vec<(String, bool, bool)> = serde_json::from_slice(include_bytes!(
        "../../../../release-version-grammar/boundaries.json"
    ))?;
    let mut before = witness::Cases::new();
    for (key, expected) in [("valid", true), ("invalid", false)] {
        for v in vectors[key].as_array().ok_or("invalid vectors")? {
            before.push((v.as_str().ok_or("invalid version")?.to_owned(), expected));
        }
    }
    let original_count = before.len();
    before.extend(boundaries.iter().map(|(s, b, _)| (s.clone(), *b)));
    let after = boundaries.iter().map(|(s, _, a)| (s.clone(), *a)).collect();
    let staged = work.join("graphs");
    fs::create_dir(&staged)?;
    let mut lanes = json!({});
    for (name, graph, module, span, cases) in [
        ("baseline", baseline, &source, span, &before),
        ("mutation", changed, &mutated, changed_span, &after),
    ] {
        let input = staged.join(format!("{name}.json"));
        fs::copy(graph.join("graph.json"), &input)?;
        let project = |input: &Path, out: &Path| {
            run(command(&work, Some(&rules), &cli)
                .args(["build", "text-grammar", "rust"])
                .arg(input)
                .arg(out))
        };
        let projection = work.join(format!("projection-{name}"));
        project(&input, &projection)?;
        if name == "baseline" {
            let mut value = load(&input)?;
            value.sort_all_objects();
            let equivalent = staged.join("reordered.json");
            fs::write(&equivalent, serde_json::to_vec(&value)?)?;
            let repeated = work.join("projection-reordered");
            project(&equivalent, &repeated)?;
            require(
                files(&projection)? == files(&repeated)?,
                "reordered equivalent graph changed output",
            )?;
        }
        let selected = &module[span.clone()];
        let native = witness::observe(
            &private,
            &format!("native-{name}"),
            selected,
            "version",
            cases,
            None,
        )?;
        let generated = witness::observe(
            &work,
            &format!("generated-{name}"),
            &fs::read_to_string(projection.join("text.rs"))?,
            "accepts",
            cases,
            Some(&rules),
        )?;
        require(native == generated, "native and projected results differ")?;
        lanes[name] = json!({"sourceSha256":bytes_sha(module.as_bytes()),"sourceSpan":span_json(&span),"enclosingNativeModule":witness::enclosing(&reference,&reference_target,module,cases)?,
            "graphSha256":sha(&input)?,"projectionLockSha256":sha(&projection.join("projection.lock"))?,"sourceOfNativeWitnessSha256":bytes_sha(selected.as_bytes()),"matchingCases":cases.len(),"outcomesSha256":generated});
    }
    let revision = git(&companion, &["rev-parse", "HEAD"])?;
    let rustc = String::from_utf8(run(Command::new("rustc").args(["+1.97.1", "-vV"]))?)?;
    let result = json!({"frameworkCommit":FRAMEWORK,"originalCommit":ORIGINAL,"companionRevision":revision.trim(),"platform":HOST,"deniedReadProbes":["originalModule","sourceControl"],"equivalentGraphEmitsIdenticalArtifacts":true,
        "authoredInputs":["stable graph IRI","entry function","two-function source selection"],"sourceSelectionSha256":bytes_sha(&selection),"sourceOriginSha256":sha(&data.join("origin.json"))?,
        "unmodifiedVectorsSha256":bytes_sha(native_vectors.as_bytes()),"unmodifiedVectors":original_count,"changedBoundaryOutcomes":boundaries.iter().filter(|(_,b,a)|b!=a).count(),
        "verifierSha256":sha(&env::current_exe()?)?,"lanes":lanes,"nativeUpstreamHarnesses":TESTS.map(|name|json!({"name":name,"passed":1})),"rustc":rustc.lines().collect::<Vec<_>>()});
    let output = std::path::absolute(args.receipt)?;
    fs::create_dir_all(output.parent().unwrap())?;
    fs::write(output, public_data_qualification::json_bytes(&result)?)?;
    Ok(())
}
