//! Add the prototype member and qualify its private reader against the pinned upstream test.
use crate::{
    assembly::{ORIGINAL, build_env, bytes_sha},
    six_composition::{compose_member, packages},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub const MEMBER: &str = "marmish-prototype";
pub const RELEASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered";
pub const SOURCE: &str = "crates/marmish-prototype/src/lib.rs";
const BASE: &str = "workspace-v7-recovered-vault-engine-journal-mdk";
const LEAF: &str = "src/group_0/__projection_0.rs";
pub const CASES: [&str; 4] = [
    "secret_request_tests::private_reader_wipes_consumed_spans",
    "buffer_contract_observations::retained_span_and_consumed_bytes",
    "buffer_contract_observations::failed_refill_retains_prior_end",
    "buffer_contract_observations::zero_output_still_reads_and_eof_can_be_polled_again",
];
fn single<T>(mut rows: impl Iterator<Item = T>, msg: &str) -> Result<T> {
    let item = rows.next().ok_or_else(|| msg.to_string())?;
    require(rows.next().is_none(), msg)?;
    Ok(item)
}
fn combined(old: &str, new: &str) -> Result<String> {
    let previous = packages(old)?;
    let added = packages(new)?;
    let named = |p: &toml::Value| p.get("name").and_then(toml::Value::as_str) == Some(MEMBER);
    let member = single(added.iter().filter(|p| named(p)), "expected one prototype")?;
    let provider = single(
        added.iter().filter(|p| !named(p)),
        "expected one shared dependency",
    )?;
    require(
        member
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .is_some_and(|deps| deps.len() == 1 && deps.first() == provider.get("name"))
            && !previous.iter().any(named),
        "unexpected prototype dependencies or duplicate predecessor member",
    )?;
    let fields = ["name", "version", "source", "checksum"];
    require(
        fields.iter().all(|k| provider.get(*k).is_some())
            && previous
                .iter()
                .filter(|p| fields.iter().all(|k| p.get(*k) == provider.get(*k)))
                .count()
                == 1,
        "provider differs from the predecessor",
    )?;
    let start = "[[package]]\nname = \"marmish-prototype\"\n";
    let following = "[[package]]\nname = \"marmish-stream\"\n";
    require(
        new.matches(start).count() == 1 && old.matches(following).count() == 1,
        "ambiguous package entry",
    )?;
    let stanza = &new[new.find(start).ok_or("missing prototype")?..];
    let end = stanza[start.len()..]
        .find("[[package]]")
        .map_or(stanza.len(), |end| end + start.len());
    let result = old.replacen(following, &format!("{}{following}", &stanza[..end]), 1);
    require(
        packages(&result)?
            .into_iter()
            .filter(|p| !named(p))
            .eq(previous),
        "predecessor Cargo package rows changed",
    )?;
    Ok(result)
}
pub fn compose(root: &Path, check: bool) -> Result<PathBuf> {
    compose_member(
        root,
        check,
        &format!("{BASE}.json"),
        RELEASE,
        "prototype-buffer-package.json",
        MEMBER,
        combined,
    )
}
pub fn check(product: &Path) -> Result<Value> {
    let file = product.join("workspace.lock");
    require(
        fs::read(&file)? == fs::read(portable().join(RELEASE).join("workspace.lock"))?,
        "buffered workspace lock changed",
    )?;
    let current = load(&file)?;
    let previous = load(&portable().join(BASE).join("workspace.lock"))?;
    let mut members = current["members"]
        .as_object()
        .ok_or("missing members")?
        .clone();
    let new = members
        .shift_remove(MEMBER)
        .ok_or("missing prototype member")?;
    require(
        members.len() == 6
            && previous["members"] == Value::Object(members)
            && new["dependencies"] == json!([]),
        "existing member seals or prototype edges changed",
    )?;
    product::check_members(product, &current, false)?;
    Ok(current)
}
fn provider(original: &Path, host: &str) -> Result<Value> {
    let meta: Value = serde_json::from_slice(&run(build_env(
        Command::new("cargo")
            .args([
                "+1.97.1",
                "metadata",
                "--locked",
                "--offline",
                "--format-version",
                "1",
                "--filter-platform",
                host,
                "--manifest-path",
            ])
            .arg(original.join("Cargo.toml")),
    ))?)?;
    let pkgs = list(&meta["packages"])?;
    let manifest = original
        .join("crates/marmish-prototype/Cargo.toml")
        .canonicalize()?;
    let krate = single(
        pkgs.iter().filter(|p| {
            p["manifest_path"]
                .as_str()
                .and_then(|s| Path::new(s).canonicalize().ok())
                .as_ref()
                == Some(&manifest)
        }),
        "ambiguous prototype metadata",
    )?;
    let node = single(
        list(&meta["resolve"]["nodes"])?
            .iter()
            .filter(|n| n["id"] == krate["id"]),
        "ambiguous prototype resolver node",
    )?;
    let edge = single(
        list(&node["deps"])?
            .iter()
            .filter(|e| e["name"] == "zeroize"),
        "ambiguous zeroize dependency",
    )?;
    let pkg = single(
        pkgs.iter().filter(|p| p["id"] == edge["pkg"]),
        "ambiguous resolved provider",
    )?;
    let identity = |path: &Path| -> Result<Value> {
        let rows = packages(&fs::read_to_string(path)?)?;
        let row = single(
            rows.iter().filter(|p| {
                p.get("name").and_then(|v| v.as_str()) == pkg["name"].as_str()
                    && p.get("version").and_then(|v| v.as_str()) == pkg["version"].as_str()
            }),
            "provider not unique in lock",
        )?;
        let mut id = json!({});
        for k in ["name", "version", "source", "checksum"] {
            id[k] = row
                .get(k)
                .and_then(|v| v.as_str())
                .ok_or("missing provider identity")?
                .into();
        }
        Ok(id)
    };
    let selected = identity(&original.join("Cargo.lock"))?;
    require(
        selected == identity(&portable().join("prototype-buffer-v1/Cargo.lock"))?
            && selected["name"] == "zeroize"
            && selected["source"] == pkg["source"],
        "original storage provider differs from released package",
    )?;
    Ok(selected)
}
pub const READER_TEST_SHA: &str =
    "f841185690a16f313b54410278ed093e80aa9078dd790c5090d698d7ea763fcc";

pub fn reader_checkout(original: &Path, work: &Path) -> Result<()> {
    run(Command::new("git")
        .args(["clone", "-q", "--no-checkout", "--local", "--shared"])
        .arg(original)
        .arg(work))?;
    run(Command::new("git")
        .current_dir(work)
        .args(["checkout", "-q", "--detach", ORIGINAL]))?;
    Ok(())
}
pub fn reader_source(original: &Path) -> Result<String> {
    stdout(
        Command::new("git")
            .current_dir(original)
            .args(["show", &format!("{ORIGINAL}:{SOURCE}")]),
    )
}

pub fn original_test(source: &str) -> Result<String> {
    let marker = "    #[test]\n    fn private_reader_wipes_consumed_spans";
    require(
        source.matches(marker).count() == 1,
        "upstream reader test changed",
    )?;
    let (_, tail) = source
        .split_once(marker)
        .ok_or("missing upstream reader test")?;
    let (body, _) = tail
        .split_once("    #[test]")
        .ok_or("missing end of upstream test")?;
    let body = format!("{marker}{body}");
    require(
        bytes_sha(body.as_bytes()) == READER_TEST_SHA,
        "upstream test digest differs",
    )?;
    Ok(format!(
        "\n#[cfg(test)]\nmod secret_request_tests {{\nuse super::*;\nuse std::io::{{self, BufRead}};\nuse zeroize::Zeroizing;\n{body}}}\n"
    ))
}
fn copy_tree(src: &Path, dst: &Path) -> Result {
    let kind = fs::symlink_metadata(src)?.file_type();
    if kind.is_dir() {
        fs::create_dir(dst)?;
        for file in fs::read_dir(src)? {
            let file = file?;
            copy_tree(&file.path(), &dst.join(file.file_name()))?;
        }
    } else {
        require(
            kind.is_file(),
            "unexpected non-regular generated workspace input",
        )?;
        fs::copy(src, dst)?;
    }
    Ok(())
}
/// Instrument a private copy so a failed or interrupted test cannot modify the sealed product.
pub(crate) fn test_workspace(product: &Path) -> Result<tempfile::TempDir> {
    let tmp = tempfile::tempdir()?;
    for file in ["Cargo.toml", "Cargo.lock"] {
        copy_tree(&product.join(file), &tmp.path().join(file))?;
    }
    for member in product::PACKAGES {
        let src = product.join(member);
        let dst = tmp.path().join(member);
        fs::create_dir(&dst)?;
        for file in ["Cargo.toml", "src"] {
            copy_tree(&src.join(file), &dst.join(file))?;
        }
        if src.join("build.rs").try_exists()? {
            copy_tree(&src.join("build.rs"), &dst.join("build.rs"))?;
        }
    }
    Ok(tmp)
}
pub fn observe(product: &Path, original: &Path, host: &str, prior: Option<&Path>) -> Result<Value> {
    require(
        [assembly::DARWIN, assembly::LINUX].contains(&host),
        "unsupported private-buffer witness host",
    )?;
    let private = whole().join("private-buffer");
    let reference = private.join(format!("original-{host}.json"));
    let standalone = private.join(format!("candidate-{host}.json"));
    let witness = load(&reference)?;
    let observations = private.join("observations.rs");
    let source = reader_source(original)?;
    require(
        witness["revision"] == ORIGINAL
            && witness["target"] == host
            && witness["passed"] == json!(CASES)
            && witness["upstreamFileSha256"] == bytes_sha(source.as_bytes())
            && witness["callerSha256"] == sha(&observations)?,
        "upstream private reader observation changed",
    )?;
    let leaf = product.join(MEMBER).join(LEAF);
    let sealed = fs::read(&leaf)?;
    let result = if let Some(prior) = prior {
        require(
            sealed == fs::read(prior.join(LEAF))?,
            "private reader differs from predecessor",
        )?;
        json!({"passed":CASES,"privateReaderLeafSha256":bytes_sha(&sealed)})
    } else {
        let package = sha(&product.join(MEMBER).join("package.lock"))?;
        let baseline = load(&standalone)?;
        let provider = provider(original, host)?;
        require(
            baseline["baseline"]["packageLockSha256"] == package
                && baseline["erasureProvider"] == provider,
            "sealed private reader differs from standalone qualification",
        )?;
        json!({"scope":"Private in-memory buffered reader","standalonePackageLockSha256":package,"originalWitnessSha256":sha(&reference)?,"standaloneGeneratedWitnessSha256":sha(&standalone)?,"observationsSha256":sha(&observations)?,"erasureProvider":provider,"passed":CASES})
    };
    let mut staged = sealed.clone();
    staged.extend(original_test(&source)?.as_bytes());
    staged.extend(fs::read(observations)?);
    let tmp = test_workspace(product)?;
    fs::write(tmp.path().join(MEMBER).join(LEAF), staged)?;
    for case in CASES {
        let exact = format!("buffer::__projection_0::{case}");
        let output = capture(
            build_env(Command::new("cargo").current_dir(tmp.path()).args([
                "+1.97.1",
                "test",
                "--release",
                "--locked",
                "--offline",
                "--lib",
                "-p",
                MEMBER,
                "--",
                "--exact",
                &exact,
            ])),
            600,
            None,
        )?;
        let out = String::from_utf8(output.stdout)?;
        require(
            output.status.success(),
            format!("{out}{}", String::from_utf8_lossy(&output.stderr)),
        )?;
        require(
            out.lines()
                .filter(|line| *line == format!("test {exact} ... ok"))
                .count()
                == 1,
            format!("missing unchanged reader case {exact}"),
        )?;
    }
    Ok(result)
}
