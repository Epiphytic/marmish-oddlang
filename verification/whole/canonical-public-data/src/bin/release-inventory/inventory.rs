use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
};
fn local(root: &Path, v: &Value) -> Result<String> {
    Ok(Path::new(string(v)?)
        .strip_prefix(root)?
        .to_str()
        .ok_or("invalid path")?
        .to_owned())
}
fn workspace(root: &Path, manifest: &str) -> Result<Value> {
    let metadata: Value = serde_json::from_str(&run(
        root,
        "cargo",
        &[
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            manifest,
        ],
    )?)?;
    let mut packages = BTreeMap::new();
    for p in list(&metadata["packages"])? {
        let mut targets = BTreeMap::new();
        for t in list(&p["targets"])? {
            let mut features = match t.get("required-features") {
                Some(v) => strings(v)?,
                None => vec![],
            };
            features.sort();
            targets.insert((strings(&t["kind"])?,string(&t["name"])?),json!({
                "name":t["name"],"kind":t["kind"],"crate_types":t["crate_types"],"path":local(root,&t["src_path"])?,
                "required_features":features,"test":t["test"],"doctest":t["doctest"],"edition":t["edition"]
            }));
        }
        packages.insert(string(&p["name"])?,json!({
            "name":p["name"],"version":p["version"],"edition":p["edition"],"minimum_rust":p["rust_version"],
            "features":p["features"],"manifest":local(root,&p["manifest_path"])?,"targets":targets.into_values().collect::<Vec<_>>()
        }));
    }
    let lock = fs::read(root.join(manifest).with_file_name("Cargo.lock"))?;
    Ok(
        json!({"manifest":manifest,"lock_sha256":format!("{:x}",Sha256::digest(lock)),"packages":packages.into_values().collect::<Vec<_>>()}),
    )
}
pub fn package<'a>(spaces: &'a Value, name: &str) -> Result<&'a Value> {
    for w in list(spaces)? {
        for p in list(&w["packages"])? {
            if p["name"] == name {
                return Ok(p);
            }
        }
    }
    Err(format!("missing package {name}").into())
}
fn matches(path: &str, patterns: &Value) -> Result<bool> {
    let path = CString::new(path)?;
    for pat in strings(patterns)? {
        let pattern = CString::new(pat)?;
        // Match whole Git paths, including slashes; backslashes are literal as in fnmatchcase.
        if unsafe { libc::fnmatch(pattern.as_ptr(), path.as_ptr(), libc::FNM_NOESCAPE) } == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn generate(repo: &Path, spec: &Value) -> Result<Value> {
    let temp = tempfile::tempdir()?;
    let repo = repo.canonicalize()?;
    run(
        temp.path(),
        "git",
        &[
            "clone",
            "--quiet",
            "--no-checkout",
            "--no-local",
            repo.to_str().ok_or("invalid path")?,
            "source",
        ],
    )?;
    let root = temp.path().join("source").canonicalize()?;
    let commit = string(&spec["commit"])?;
    run(&root, "git", &["checkout", "--quiet", "--detach", commit])?;
    if run(&root, "git", &["rev-parse", "HEAD"])?.trim() != commit {
        return Err("revision differs from the release pin".into());
    }
    let manifests = strings(&spec["workspaces"])?;
    let workspaces = json!(
        manifests
            .iter()
            .map(|m| workspace(&root, m))
            .collect::<Result<Vec<_>>>()?
    );
    let files = run(&root, "git", &["ls-files", "-z"])?;
    let tracked = files.split_terminator('\0').collect::<BTreeSet<_>>();
    let mut covered = manifests.into_iter().collect::<BTreeSet<_>>();
    for w in list(&workspaces)? {
        for p in list(&w["packages"])? {
            covered.insert(string(&p["manifest"])?);
        }
    }
    let actual = tracked
        .iter()
        .copied()
        .filter(|p| Path::new(p).file_name().is_some_and(|f| f == "Cargo.toml"))
        .collect::<BTreeSet<_>>();
    if actual != covered {
        return Err(format!(
            "unclassified Cargo manifests: {:?}",
            actual.symmetric_difference(&covered).collect::<Vec<_>>()
        )
        .into());
    }
    let surfaces = list(&spec["surfaces"])?;
    let roots = strings(&spec["inventory_roots"])?;
    let mut groups = BTreeMap::<&str, Vec<&str>>::new();
    for rule in surfaces {
        groups.insert(string(&rule["id"])?, vec![]);
    }
    for file in files.split_terminator('\0') {
        if !roots.contains(&file.split('/').next().unwrap()) {
            continue;
        }
        let mut classified = false;
        for rule in surfaces {
            if matches(file, &rule["paths"])? {
                groups.get_mut(string(&rule["id"])?).unwrap().push(file);
                classified = true;
                break;
            }
        }
        if !classified {
            return Err(format!("unclassified input {file}").into());
        }
    }
    let release = &spec["release"];
    let mut witnesses = strings(&release["witnesses"])?;
    for p in list(&release["external_programs"])? {
        witnesses.push(string(&p["witness"])?);
    }
    for w in witnesses {
        if !tracked.contains(w) {
            return Err(format!("missing release witness {w}").into());
        }
    }
    let platforms = object(&release["platforms"])?
        .keys()
        .collect::<BTreeSet<_>>();
    let mut cells = vec![];
    for (profile, features) in object(&release["profiles"])?
        .iter()
        .collect::<BTreeMap<_, _>>()
    {
        for f in strings(features)? {
            let (owner, key) = f.split_once('/').ok_or("invalid feature reference")?;
            if package(&workspaces, owner)?["features"].get(key).is_none() {
                return Err(format!("unknown feature {f}").into());
            }
        }
        for target in &platforms {
            cells.push(json!({"profile":profile,"target":target,"cargo_features":features}));
        }
    }
    let owner = package(&workspaces, string(&release["package"])?)?;
    let mut assets = vec![];
    for platform in platforms {
        for template in strings(&release["payloads"])? {
            assets.push(
                template
                    .replace("{target}", platform)
                    .replace("{version}", string(&owner["version"])?),
            );
        }
    }
    assets.sort();
    let toolchain: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("rust-toolchain.toml"))?)?;
    let changes = run(
        &root,
        "git",
        &[
            "diff",
            "--name-only",
            "-z",
            string(&spec["previous_commit"])?,
            commit,
        ],
    )?;
    Ok(
        json!({"source":spec["source"],"commit":commit,"previous_commit":spec["previous_commit"],
        "changed_paths":changes.split_terminator('\0').collect::<Vec<_>>(),"tree":run(&root,"git",&["rev-parse","HEAD^{tree}"])?.trim(),
        "toolchain":toolchain["toolchain"],"workspaces":workspaces,"file_groups":groups,"platform_contracts":release["platforms"],
        "baseline_cells":cells,"release_assets":assets}),
    )
}
