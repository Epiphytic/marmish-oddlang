use clap::Parser;
use public_data_qualification::{
    Result, capture, json_bytes, load, pin, portable, require, run, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
};
#[path = "../../../release-info-structure/ingest.rs"]
mod ingest;

const REV: &str = "af54339d250290490d62d4475a77539b7e739071";
const ENGINE: &str = "crates/marmish-engine/src/store.rs";
const GRAPH: &str = "native-build-report-recovered-v1/graph.json";
const PACKAGE: &str = "native-build-report-recovered-package.json";
const WORKSPACE: &str = "workspace-v7-recovered-report.json";
const ROLES: [&str; 2] = ["marmish", "marmishd"];

#[derive(Parser)]
struct Args {
    oddlang: PathBuf,
    original: PathBuf,
    output: PathBuf,
    #[arg(long)]
    framework_revision: String,
    #[arg(long)]
    original_target: PathBuf,
    #[arg(long)]
    generated_target: PathBuf,
    /// Authenticate already-built upstream metadata witnesses using the supplied digest manifest.
    #[arg(long)]
    retained_original_pins: Option<PathBuf>,
}
fn write(p: &Path, v: &Value) -> Result {
    Ok(fs::write(p, json_bytes(v)?)?)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    stdout(Command::new("git").current_dir(root).args(args))
}
fn compose(base: &Path, recovered: &Path) -> Result<PathBuf> {
    let mut graph = load(&recovered.join("graph.json"))?;
    graph["guards"] = load(&base.join("native-build-report-v1/graph.json"))?["guards"].clone();
    write(&base.join(GRAPH), &graph)?;
    let mut package = load(&base.join("native-build-report-package.json"))?;
    let mut matches = package["components"]
        .as_array_mut()
        .ok_or("invalid package")?
        .iter_mut()
        .filter(|c| c["kind"] == "build-report");
    matches.next().ok_or("missing report")?["graph"] = pin(base, GRAPH)?;
    require(matches.next().is_none(), "duplicate report")?;
    write(&base.join(PACKAGE), &package)?;
    let mut workspace = load(&base.join("workspace-v7.json"))?;
    workspace["members"]["marmish-native"]["manifest"] = pin(base, PACKAGE)?;
    let path = base.join(WORKSPACE);
    write(&path, &workspace)?;
    Ok(path)
}
fn checkout(source: &Path, out: &Path) -> Result {
    run(Command::new("git")
        .args(["clone", "--shared", "--no-checkout"])
        .arg(source)
        .arg(out))?;
    git(out, &["checkout", "--detach", REV])?;
    let path = out.join(ENGINE);
    let contents = fs::read_to_string(&path)?;
    let old = "pub(crate) const ENGINE_FORMAT: i64 = 1;";
    require(
        contents.matches(old).count() == 1,
        "engine declaration changed",
    )?;
    fs::write(
        path,
        contents.replace(old, "pub(crate) const ENGINE_FORMAT: i64 = 4294967305;"),
    )?;
    let mut cmd = Command::new("git");
    for role in ["AUTHOR", "COMMITTER"] {
        cmd.env(format!("GIT_{role}_NAME"), "Qualification Fixture")
            .env(format!("GIT_{role}_EMAIL"), "fixture@example.invalid")
            .env(format!("GIT_{role}_DATE"), "2026-11-26T12:00:00Z");
    }
    run(cmd.current_dir(out).args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--only",
        "-m",
        "Change the reported engine format",
        "--",
        ENGINE,
    ]))?;
    require(
        git(out, &["diff", "--name-only", REV])?.trim() == ENGINE,
        "unexpected changed paths",
    )
}
fn cargo(root: &Path) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("+1.97.1").current_dir(root);
    for k in [
        "MARMISH_BUILD_COMMIT",
        "MARMISH_BUILD_TARGET",
        "CARGO_BUILD_TARGET",
    ] {
        cmd.env_remove(k);
    }
    cmd
}
fn lease(target: &Path) -> Result<File> {
    fs::create_dir_all(target)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(target.join(".release-info-structure.lock"))?;
    file.lock()?;
    Ok(file)
}
fn build(root: &Path, target: &Path, destination: &Path, original: bool) -> Result {
    let _lock = lease(target)?;
    fs::create_dir(destination)?;
    let mut cmd = cargo(root);
    cmd.args(["build", "--locked", "--offline", "-p", "marmish-native"]);
    let mut roles = ROLES.to_vec();
    if original {
        cmd.env("MARMISH_BUILD_COMMIT", REV);
        roles.push("marmish-worker");
    }
    for role in &roles {
        cmd.arg("--bin").arg(role);
    }
    run(cmd.arg("--target-dir").arg(target))?;
    for role in roles {
        fs::copy(target.join("debug").join(role), destination.join(role))?;
    }
    Ok(())
}
fn types(root: &Path, target: &Path, probe: &Path) -> Result {
    let _lock = lease(target)?;
    let path = root.join("crates/marmish-native/src/lib.rs");
    let before = fs::read(&path)?;
    let mut code = before.clone();
    code.push(b'\n');
    code.extend(fs::read(probe)?);
    fs::write(&path, code)?;
    let result = run(cargo(root)
        .env("MARMISH_BUILD_COMMIT", REV)
        .args([
            "check",
            "--locked",
            "--offline",
            "-p",
            "marmish-native",
            "--lib",
            "--target-dir",
        ])
        .arg(target));
    fs::write(path, before)?;
    result.map(|_| ())
}
fn observe(bins: &Path, cwd: &Path) -> Result<Value> {
    let mut records = json!({});
    for role in ROLES {
        let output = capture(
            Command::new(bins.join(role))
                .arg("release-info")
                .current_dir(cwd)
                .env_clear()
                .env("HOME", cwd)
                .env("TMPDIR", cwd)
                .env("PATH", "/bin:/usr/bin")
                .env("LANG", "C")
                .envs(
                    [
                        "TARGET",
                        "CARGO_PKG_VERSION",
                        "MARMISH_BUILD_TARGET",
                        "MARMISH_BUILD_COMMIT",
                    ]
                    .map(|k| (k, "runtime-override")),
                ),
            25,
            None,
        )?;
        require(
            output.status.success() && output.stderr.is_empty() && output.stdout.ends_with(b"\n"),
            format!("{role} release-info exit or streams changed"),
        )?;
        records[role] = json!(String::from_utf8(output.stdout)?);
    }
    require(
        fs::read_dir(cwd)?.next().is_none(),
        "metadata command created files",
    )?;
    Ok(records)
}
fn main() -> Result {
    let a = Args::parse();
    let source = a.original.canonicalize()?;
    let cli = a.oddlang.canonicalize()?;
    let here = whole().join("release-info-structure");
    fs::create_dir_all(&a.original_target)?;
    fs::create_dir_all(&a.generated_target)?;
    let left = a.original_target.canonicalize()?;
    let right = a.generated_target.canonicalize()?;
    require(left != right, "target roots overlap")?;
    let pins = a
        .retained_original_pins
        .as_ref()
        .map(|p| load(p))
        .transpose()?;
    require(
        git(&source, &["rev-parse", "HEAD"])?.trim() == REV
            && git(&source, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "upstream revision or tracked state changed",
    )?;
    fs::create_dir(&a.output)?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("no rustc host")?;
    let probe = here.join("types.rs");
    let mut receipt = json!({"format":"marmish-release-info-structure-v1","frameworkRevision":a.framework_revision,"rustc":rust.lines().collect::<Vec<_>>(),"representedRevision":REV,"nativeTypeProbeSha256":sha(&probe)?,"lanes":{}});
    let tmp = tempfile::Builder::new().prefix("rri-").tempdir()?;
    let root = tmp.path().canonicalize()?;
    let mutant = root.join("mutant");
    checkout(&source, &mutant)?;
    let mut graphs = vec![];
    let mut reports = vec![];
    for (label, src) in [("baseline", &source), ("mutant", &mutant)] {
        let recovered = root.join(format!("extracted-{label}"));
        let selection = root.join(format!("{label}-selection.json"));
        ingest::recover(&cli, src, &recovered, &selection)?;
        let evidence = load(&recovered.join("observation.json"))?;
        let base = root.join(format!("portable-{label}"));
        run(Command::new("cp").arg("-R").arg(portable()).arg(&base))?;
        let workspace = compose(&base, &recovered)?;
        if label == "baseline" {
            require(
                fs::read(&selection)? == fs::read(here.join("selection.json"))?,
                "selection drift",
            )?;
            for rel in [GRAPH, PACKAGE, WORKSPACE] {
                require(
                    fs::read(base.join(rel))? == fs::read(portable().join(rel))?,
                    format!("composition drift in {rel}"),
                )?;
            }
        }
        let generated = root.join(format!("workspace-{label}"));
        run(Command::new(&cli)
            .env("RUSTUP_TOOLCHAIN", "1.97.1")
            .args(["build", "native-workspace", "rust"])
            .arg(&workspace)
            .arg(&generated))?;
        let originals = if let Some(manifest) = &pins {
            let dir = left.join(label).join("debug");
            for role in ROLES {
                require(
                    sha(&dir.join(role))? == text(&manifest[label][role])?,
                    format!("retained {label} {role} digest mismatch"),
                )?;
            }
            dir
        } else {
            let dir = root.join(format!("original-{label}"));
            build(src, &left.join(label), &dir, true)?;
            dir
        };
        let bins = root.join(format!("candidate-{label}"));
        build(&generated, &right.join(label), &bins, false)?;
        let cwd = root.join(format!("invocation-{label}"));
        fs::create_dir(&cwd)?;
        let expected = observe(&originals, &cwd)?;
        require(
            observe(&bins, &cwd)? == expected && expected["marmish"] == expected["marmishd"],
            format!("{label} original and generated output differ"),
        )?;
        let report: Value = serde_json::from_str(text(&expected["marmish"])?)?;
        let fields = report
            .as_object()
            .ok_or("expected report object")?
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        require(
            fields == ["schema", "version", "target", "commit", "formats"],
            "report field order changed",
        )?;
        let manifest: toml::Value =
            fs::read_to_string(src.join("crates/marmish-native/Cargo.toml"))?.parse()?;
        require(
            report["target"] == host
                && report["version"].as_str() == manifest["package"]["version"].as_str()
                && report["schema"] == 1
                && report["schema"] == evidence["values"][0]
                && report["commit"] == REV
                && Some(&report["formats"])
                    == evidence["values"].as_array().and_then(|items| items.last()),
            "observed report disagrees with source",
        )?;
        let output = a.output.join(format!("{label}.json"));
        write(&output, &expected)?;
        receipt["lanes"][label] = json!({"sourceRevision":git(src,&["rev-parse","HEAD"])?.trim(),"selectionSha256":sha(&selection)?,"sourceObservation":evidence,"composedGraphSha256":sha(&base.join(GRAPH))?,"workspaceSpecSha256":sha(&workspace)?,"workspaceLockSha256":sha(&generated.join("workspace.lock"))?,"nativeOutputSha256":sha(&output)?});
        reports.push(report);
        graphs.push(load(&recovered.join("graph.json"))?);
    }
    require(
        reports[0]["formats"] == json!([2, 7, 1, 1, 1])
            && reports[1]["formats"] == json!([2, 7, 9, 1, 1]),
        "wrong u32 conversion",
    )?;
    reports[0]["formats"][2] = json!(9);
    require(reports[0] == reports[1], "unrelated report mutation")?;
    let fields = graphs[0]["fields"].as_array_mut().ok_or("no fields")?;
    let field = fields.last_mut().ok_or("empty fields")?;
    let leaf = field["source"]["items"][2]["chain"]
        .as_array_mut()
        .and_then(|c| c.last_mut())
        .ok_or("missing alias chain")?;
    require(leaf["value"] == 1, "wrong engine alias source")?;
    leaf["value"] = json!(4294967305u64);
    require(graphs[0] == graphs[1], "unrelated structural changes")?;
    let check_target = if pins.is_some() {
        right.join("source-type-check")
    } else {
        left.join("mutant")
    };
    types(&mutant, &check_target, &probe)?;
    for checkout in [&source, &mutant] {
        require(
            git(checkout, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
            "tracked source modified",
        )?;
    }
    write(&a.output.join("receipt.json"), &receipt)
}
