//! Project the published byte width and exercise the independent Node witness in Seatbelt.
use clap::Parser;
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, root},
    capture, json_bytes, list, load, require, run, sha, stdout, whole,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const FRAMEWORK: &str = "0f8410636148e3afa96005c9c24390fbd32eb602";
const GRAPH: &str = "f87cf4fad9f37189dd02c5a4179e90017380bfbc8affb867eab5fc23a2d6047b";
const VALUE: &str = "urn:marmish:vault:secret";
const FAILURE: &str = "urn:marmish:vault:error:malformed";
const OUTPUTS: [&str; 3] = ["obligations.json", "projection.lock", "values.mts"];

#[derive(Parser)]
struct Args {
    framework: PathBuf,
    compiler_target: PathBuf,
    receipt: PathBuf,
}

struct Sandbox {
    work: PathBuf,
    profile: String,
}
impl Sandbox {
    fn new(work: PathBuf, framework: &Path) -> Result<Self> {
        let mut profile = String::from("(version 1)(allow default)(deny network*)");
        let home = env::home_dir().ok_or("missing home directory")?;
        for path in [
            root().canonicalize()?,
            framework.into(),
            "/private/tmp".into(),
            home.join("repos"),
        ] {
            require(
                !work.starts_with(&path),
                "scratch directory overlaps a denied path",
            )?;
            profile.push_str(&format!(
                "(deny file-read* (subpath {}))",
                serde_json::to_string(&path)?
            ));
        }
        Ok(Self { work, profile })
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-p")
            .arg(&self.profile)
            .current_dir(&self.work)
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_COMPILE_CACHE")
            .env("TMPDIR", &self.work);
        cmd
    }
    fn project(
        &self,
        compiler: &Path,
        graph: &Path,
        output: &Path,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        run(self
            .command()
            .arg(compiler)
            .args(["build", "checked-values", "typescript"])
            .arg(graph)
            .arg(output))?;
        let mut artifacts = BTreeMap::new();
        for entry in fs::read_dir(output)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF-8 artifact name")?;
            require(
                OUTPUTS.contains(&name.as_str()) && entry.file_type()?.is_file(),
                "unexpected projection artifact",
            )?;
            artifacts.insert(name, fs::read(entry.path())?);
        }
        require(artifacts.len() == OUTPUTS.len(), "incomplete projection")?;
        Ok(artifacts)
    }
    fn observe(&self, witness: &Path, output: &Path, width: u64) -> Result<Value> {
        let result: Value = serde_json::from_slice(&run(self
            .command()
            .args(["node", "--experimental-strip-types"])
            .arg(witness)
            .arg(output.join("values.mts"))
            .arg(VALUE)
            .arg(width.to_string())
            .arg(FAILURE))?)?;
        require(
            result
                == json!({"short":FAILURE,"long":FAILURE,"shadowedWidth":FAILURE,"detached":FAILURE,
            "owned":{"id":VALUE,"width":width,"inputCopy":true,"outputCopy":true},"crossRealm":true}),
            "Node observation differs from the independent width and ownership expectations",
        )?;
        Ok(result)
    }
}

fn main() -> Result {
    let args = Args::parse();
    require(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "requires native ARM64 macOS",
    )?;
    let framework = args.framework.canonicalize()?;
    let git = |repo: &Path, args: &[&str]| stdout(Command::new("git").current_dir(repo).args(args));
    require(
        git(&framework, &["rev-parse", "HEAD"])?.trim() == FRAMEWORK
            && git(
                &framework,
                &["status", "--porcelain", "--untracked-files=no"],
            )?
            .is_empty(),
        "expected clean tracked compiler sources at pinned revision",
    )?;
    require(
        git(
            root(),
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
        "dirty qualification inputs",
    )?;
    let data = root().join("oddlang/portable/vault-fixed-bytes-v1");
    let raw = fs::read(data.join("graph.json"))?;
    require(bytes_sha(&raw) == GRAPH, "wrong vault byte-width graph")?;
    let target = std::path::absolute(args.compiler_target)?;
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
        .arg(&target))?;
    let parent = stdout(Command::new("/usr/bin/getconf").arg("DARWIN_USER_TEMP_DIR"))?;
    let temp = tempfile::tempdir_in(parent.trim())?;
    let sandbox = Sandbox::new(temp.path().canonicalize()?, &framework)?;
    let work = &sandbox.work;
    for path in [
        data.join("rust.json"),
        whole().join("unified-recovered-vault/fixed_bytes.rs"),
    ] {
        require(path.is_file(), "denial probe missing")?;
        let r = capture(sandbox.command().arg("/bin/cat").arg(path), 30, None)?;
        require(
            !r.status.success()
                && String::from_utf8_lossy(&r.stderr).contains("Operation not permitted"),
            "sandbox did not deny a Rust input",
        )?;
    }
    let tools = work.join("tools");
    let inputs = work.join("inputs");
    fs::create_dir(&tools)?;
    fs::create_dir(&inputs)?;
    let cli = tools.join("oddlang");
    fs::copy(target.join("debug/oddlang"), &cli)?;
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755))?;
    let graph = inputs.join("domain.json");
    fs::write(&graph, &raw)?;
    require(
        fs::read_dir(&inputs)?.count() == 1,
        "expected only the portable graph",
    )?;
    let baseline = work.join("baseline");
    let artifacts = sandbox.project(&cli, &graph, &baseline)?;
    let mut definition: Value = serde_json::from_slice(&raw)?;
    let mut sorted = definition.clone();
    sorted.sort_all_objects();
    fs::write(&graph, serde_json::to_vec(&sorted)?)?;
    require(
        artifacts == sandbox.project(&cli, &graph, &work.join("reordered"))?,
        "reordered graph changed output",
    )?;
    definition["types"][VALUE]["length"] = 31.into();
    fs::write(&graph, json_bytes(&definition)?)?;
    let mutation = work.join("mutation");
    sandbox.project(&cli, &graph, &mutation)?;
    let witness = tools.join("witness.mjs");
    fs::copy(
        framework.join("verification/checked-values/typescript.mjs"),
        &witness,
    )?;
    let obligations: Value = serde_json::from_slice(&artifacts["obligations.json"])?;
    let [value] = list(&obligations["obligations"])?.as_slice() else {
        return Err("expected one byte-width obligation".into());
    };
    let disposal = &value["disposal"];
    require(
        value["declaration"] == VALUE
            && value["expected"]
                == json!({"length":32,"failure":FAILURE,"input":"exact-bytes","ownsInputCopy":true,"view":"copy","onDrop":"erase"})
            && disposal["qualification"] == "unsupported"
            && disposal["reason"].as_str().is_some_and(|s| !s.is_empty()),
        "unexpected vault obligation",
    )?;
    let lock: Value = serde_json::from_slice(&artifacts["projection.lock"])?;
    let hashes: BTreeMap<_, _> = artifacts
        .iter()
        .map(|(k, v)| (k.clone(), bytes_sha(v)))
        .collect();
    require(
        lock.get("bindingSha256").is_none()
            && lock["sourceSha256"] == hashes["values.mts"]
            && lock["obligationsSha256"] == hashes["obligations.json"],
        "projection lock differs",
    )?;
    let native = whole().join("vault-fixed-bytes/aarch64-apple-darwin.json");
    require(
        load(&native)?["graphSha256"] == GRAPH,
        "native width witness has a different graph",
    )?;
    let result = json!({"frameworkCommit":FRAMEWORK,"companionRevision":git(root(),&["rev-parse","HEAD"])?.trim(),"node":stdout(sandbox.command().args(["node","--version"]))?.trim(),
        "platform":"aarch64-apple-darwin","graphSha256":GRAPH,"nativeGraphQualificationSha256":sha(&native)?,"compilerSha256":sha(&cli)?,
        "verifierSha256":sha(&env::current_exe()?)?,"nodeWitnessSha256":sha(&witness)?,"deniedReadProbes":["rustBinding","rustWitness"],
        "inputFiles":["domain.json"],"artifactSha256":hashes,"reorderedGraphEmitsIdenticalArtifacts":true,
        "baseline":sandbox.observe(&witness,&baseline,32)?,"widthMutation":sandbox.observe(&witness,&mutation,31)?,"disposal":disposal});
    let receipt = std::path::absolute(args.receipt)?;
    fs::create_dir_all(receipt.parent().ok_or("missing receipt parent")?)?;
    fs::write(receipt, json_bytes(&result)?)?;
    Ok(())
}
