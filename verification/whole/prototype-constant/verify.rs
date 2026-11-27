//! Check the public constant's native caller, source mutation and workspace delta.
use crate::{
    assembly::{ORIGINAL, archive, build_env},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const RELEASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit";
pub const CALLER: &str = "prototype-constant/caller.rs";
const CONTRACT: &str = "prototype-constant-v1";

pub fn predecessor(cli: &Path, scratch: &Path) -> Result<PathBuf> {
    let prior = scratch.join("prior");
    run(Command::new(cli)
        .args(["build", "native-package", "rust"])
        .arg(portable().join("prototype-buffer-package.json"))
        .arg(&prior))?;
    Ok(prior)
}

pub fn check(product: &Path, prior: &Path) -> Result<Value> {
    let lock = product.join("workspace.lock");
    require(
        fs::read(&lock)? == fs::read(portable().join(RELEASE).join("workspace.lock"))?,
        "successor workspace lock differs",
    )?;
    let current = load(&lock)?;
    let previous = load(&portable().join(buffered::RELEASE).join("workspace.lock"))?;
    require(
        current["cargoLockSha256"] == previous["cargoLockSha256"],
        "Cargo provider pins changed",
    )?;
    let siblings = |r: &Value| -> Result<Value> {
        let mut rows = r["members"].as_object().ok_or("missing members")?.clone();
        rows.remove(buffered::MEMBER).ok_or("missing prototype")?;
        require(rows.len() == 6, "expected six siblings")?;
        Ok(Value::Object(rows))
    };
    require(
        siblings(&current)? == siblings(&previous)?,
        "sibling lock changed",
    )?;
    let prior_lock = prior.join("package.lock");
    require(
        sha(&prior_lock)? == previous["members"][buffered::MEMBER]["standalonePackageLockSha256"],
        "prior package lock differs",
    )?;
    for (path, digest) in load(&prior_lock)?["generatedFiles"]
        .as_object()
        .ok_or("missing predecessor inventory")?
    {
        if path != "src/lib.rs" {
            require(
                sha(&product.join(buffered::MEMBER).join(path))? == *digest,
                format!("private file changed: {path}"),
            )?;
        }
    }
    product::check_members(product, &current, false)?;
    Ok(current)
}

pub fn caller(library: &Path, executable: &Path) -> Result<Value> {
    let parent = library.parent().ok_or("missing library directory")?;
    let deps = parent.join("deps");
    let deps = if deps.is_dir() {
        deps.as_path()
    } else {
        parent
    };
    run(Command::new("rustc")
        .args(["+1.97.1", "--edition=2021"])
        .arg(whole().join(CALLER))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("marmish_prototype={}", library.display()))
        .arg("-o")
        .arg(executable))?;
    let out = stdout(&mut Command::new(executable))?;
    require(out == "131072\n", "MAX_LINE differs")?;
    Ok(json!({"librarySha256":sha(library)?,"stdout":out}))
}

fn recover(root: &Path, cli: &Path, target: &Path, scratch: &Path, tag: &str) -> Result<PathBuf> {
    run(build_env(
        Command::new("cargo")
            .current_dir(root)
            .args([
                "+1.97.1",
                "rustdoc",
                "--release",
                "--locked",
                "--offline",
                "--target-dir",
            ])
            .arg(target)
            .args([
                "-p",
                buffered::MEMBER,
                "--lib",
                "--",
                "-Z",
                "unstable-options",
                "--output-format",
                "json",
            ]),
    )
    .env("RUSTC_BOOTSTRAP", "1"))?;
    let src = root.join(buffered::SOURCE);
    let doc = target.join("doc/marmish_prototype.json");
    let selection = scratch.join(format!("{tag}-selection.json"));
    fs::write(
        &selection,
        json_bytes(&json!({"format":"oddlang-public-constant-selection-v1",
      "id":"urn:marmish:prototype:limits","constantId":"urn:marmish:prototype:max-line",
      "crateName":"marmish_prototype","name":"MAX_LINE","sourceFilename":buffered::SOURCE,
      "sourceSha256":sha(&src)?,"rustdocSha256":sha(&doc)?}))?,
    )?;
    let output = scratch.join(tag);
    run(Command::new(cli)
        .args(["recover", "public-constant"])
        .arg(src)
        .arg(doc)
        .arg(selection)
        .arg(&output))?;
    Ok(output)
}

pub fn source_oracle(original: &Path, cli: &Path, target: &Path, scratch: &Path) -> Result<Value> {
    let base = recover(original, cli, target, scratch, "baseline")?;
    for file in ["graph.json", "rust.json"] {
        require(
            fs::read(base.join(file))? == fs::read(portable().join(CONTRACT).join(file))?,
            format!("baseline {file} differs"),
        )?;
    }
    let changed = scratch.join("changed");
    archive(original, ORIGINAL, &changed)?;
    let source = changed.join(buffered::SOURCE);
    let content = fs::read_to_string(&source)?;
    let before = "pub const MAX_LINE: usize = 128 * 1024;";
    require(
        content.matches(before).count() == 1,
        "expected one selected expression",
    )?;
    fs::write(
        source,
        content.replace(before, "pub const MAX_LINE: usize = 129 * 1024;"),
    )?;
    let mutation = recover(&changed, cli, target, scratch, "mutation")?;
    require(
        load(&mutation.join("graph.json"))?["constants"]
            == json!([{"id":"urn:marmish:prototype:max-line","value":{"multiply":[129,1024]}}]),
        "mutated expression differs",
    )?;
    require(
        fs::read(mutation.join("rust.json"))? == fs::read(base.join("rust.json"))?,
        "value mutation changed binding",
    )?;
    let old = load(&base.join("recovery.lock"))?;
    let new = load(&mutation.join("recovery.lock"))?;
    require(
        old["unsignedDecimal"] == "131072" && new["unsignedDecimal"] == "132096",
        "native numeric oracle differs",
    )?;
    Ok(json!({"original":old,"mutation":new}))
}
