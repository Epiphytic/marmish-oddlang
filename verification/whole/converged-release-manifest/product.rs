//! Compose the crate-private line grammar and prove the predecessor native package seal.
use crate::{
    api_composition::{Descriptor, set_member},
    assembly::bytes_sha,
    *,
};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const BASE: &str = "workspace-v7-converged-public-api";
pub const OUTPUT: &str = "workspace-v7-converged-release-manifest";
pub const MEMBER: &str = "marmish-native";
pub const SOURCE: &str = "crates/marmish-native/src/native_manifest.rs";
pub const VECTOR: &str = "tools/release-version-vectors.json";
pub const CASES: [&str; 2] = ["strict_vectors", "shared_release_version_vectors"];
pub const BINDING: &str = "release-manifest-integrated-v1/rust.json";
pub const GRAPH: &str = "release-manifest-v1/graph.json";
const BASE_LOCK: &str = "3ed25cad2b1399e1c63787fedca0abbb465e758c9cfb6c635b17188a40b2dfaa";
pub(crate) fn descriptors(root: &Path) -> Result<Vec<Descriptor>> {
    require(
        sha(&root.join(BASE).join("workspace.lock"))? == BASE_LOCK,
        "wrong published predecessor",
    )?;
    let mut workspace = load(&root.join(format!("{BASE}.json")))?;
    let descriptor = &workspace["members"][MEMBER]["manifest"];
    let name = text(&descriptor["path"])?;
    require(
        pin(root, name)? == *descriptor,
        "native predecessor manifest changed",
    )?;
    let mut native = load(&root.join(name))?;
    let parts = native
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("missing native components")?;
    for part in parts.iter() {
        require(
            text(&part["module"])? != "native_manifest",
            "predecessor already has a parser",
        )?;
    }
    let grammar = load(&root.join(GRAPH))?;
    let [version] = list(&grammar["textGrammars"])?.as_slice() else {
        return Err("expected one version text grammar".into());
    };
    let mut overlay = load(&root.join("release-manifest-v1/rust.json"))?;
    overlay.as_object_mut().ok_or("invalid Rust binding")?.insert("support".into(),json!({"nestedGrammars":{(text(&version["id"])?):"version"},"textChoices":{"target":"TARGETS"}}));
    let binding = Descriptor::new(BINDING, &overlay)?;
    parts.push(json!({"kind":"line-grammar","module":"native_manifest","graph":pin(root,GRAPH)?,"overlay":binding.pin(),"reexports":[],"crateReexports":["Manifest","Asset","version","TARGETS"]}));
    let package = Descriptor::new("native-converged-release-manifest-package.json", &native)?;
    set_member(&mut workspace, MEMBER, &package)?;
    Ok(vec![
        binding,
        package,
        Descriptor::new("workspace-v7-converged-release-manifest.json", &workspace)?,
    ])
}
fn capture_one<'a>(pattern: &str, source: &'a str) -> Result<&'a str> {
    let re = Regex::new(pattern)?;
    let mut matches = re.captures_iter(source);
    let first = matches.next().ok_or("missing private parser wrapper")?;
    require(matches.next().is_none(), "ambiguous private parser wrapper")?;
    Ok(first.get(1).ok_or("invalid private wrapper")?.as_str())
}
pub fn leaf(native: &Path) -> Result<(PathBuf, String)> {
    let source = fs::read_to_string(native.join("src/lib.rs"))?;
    let group = capture_one(
        r#"(?m)^#\[path = "(group_\d+)/mod\.rs"\]\nmod native_manifest;$"#,
        &source,
    )?;
    let wrapper = native.join("src").join(group);
    let source = fs::read_to_string(wrapper.join("mod.rs"))?;
    let name = capture_one(r"(?m)^mod (__projection_\d+);$", &source)?;
    Ok((
        wrapper.join(format!("{name}.rs")),
        format!("native_manifest::{name}"),
    ))
}
pub fn check(product: &Path) -> Result<Value> {
    let emitted = product.join("workspace.lock");
    require(
        fs::read(&emitted)? == fs::read(portable().join(OUTPUT).join("workspace.lock"))?,
        "integrated projection lock differs",
    )?;
    let before = portable().join(BASE).join("workspace.lock");
    require(
        sha(&before)? == BASE_LOCK,
        "wrong predecessor workspace hash",
    )?;
    let previous = load(&before)?;
    let current = load(&emitted)?;
    for k in ["format", "cargoLockSha256", "workspaceCargoManifestSha256"] {
        require(current[k] == previous[k], format!("changed workspace {k}"))?;
    }
    let old = &previous["members"][MEMBER];
    let selected = &current["members"][MEMBER];
    let mut siblings = current["members"]
        .as_object()
        .ok_or("missing members")?
        .clone();
    siblings
        .shift_remove(MEMBER)
        .ok_or("missing native member")?;
    let mut prior = previous["members"]
        .as_object()
        .ok_or("missing predecessor members")?
        .clone();
    prior
        .shift_remove(MEMBER)
        .ok_or("missing predecessor native")?;
    require(
        siblings.len() == 6
            && siblings == prior
            && selected["dependencies"] == old["dependencies"]
            && selected["workspaceCargoManifestSha256"] == old["workspaceCargoManifestSha256"],
        "native edges, Cargo manifest or siblings changed",
    )?;
    product::check_members(product, &current, false)?;
    let native = product.join(MEMBER);
    let (leaf, _) = leaf(&native)?;
    let group = leaf
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .ok_or("invalid wrapper path")?;
    let mut package = load(&native.join("package.lock"))?;
    let appended = package
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .and_then(Vec::pop)
        .ok_or("missing appended parser")?;
    require(
        appended["module"] == "native_manifest" && appended["kind"] == "line-grammar",
        "expected an appended parser component",
    )?;
    let declaration = format!("#[path = \"{group}/mod.rs\"]\nmod native_manifest;\n");
    let source = fs::read_to_string(native.join("src/lib.rs"))?;
    require(
        source.matches(&declaration).count() == 1,
        "expected one private parser wrapper declaration",
    )?;
    let files = package
        .get_mut("generatedFiles")
        .and_then(Value::as_object_mut)
        .ok_or("missing generated files")?;
    let prefix = format!("src/{group}/");
    files.retain(|k, _| !k.starts_with(&prefix));
    files.insert(
        "src/lib.rs".into(),
        bytes_sha(source.replace(&declaration, "").as_bytes()).into(),
    );
    package["manifestSha256"] = old["inputSha256"].clone();
    package.sort_all_objects();
    require(
        bytes_sha(&serde_json::to_vec_pretty(&package)?) == old["standalonePackageLockSha256"],
        "restored native wrapper or inherited files differ from predecessor",
    )?;
    Ok(current)
}
pub fn tests(original: &Path) -> Result<Vec<u8>> {
    let raw = stdout(
        Command::new("git")
            .current_dir(original)
            .args(["show", &format!("{}:{SOURCE}", assembly::ORIGINAL)]),
    )?;
    let start = "#[cfg(test)]\nmod tests {";
    let end = "    #[test]\n    fn versioned_directory_discovery_supports_both_families_and_rejects_ambiguity()";
    require(
        raw.matches(start).count() == 1 && raw.matches(end).count() == 1,
        "unexpected upstream test selection",
    )?;
    let (_, tail) = raw
        .split_once(start)
        .ok_or("missing upstream test module")?;
    let (body, _) = tail
        .split_once(end)
        .ok_or("unexpected upstream test coordinates")?;
    let body = format!("{start}{body}}}\n");
    let re = Regex::new(r"    #\[test\]\n    fn ([a-z_]+)\(")?;
    require(
        re.captures_iter(&body).map(|c| c[1].to_string()).eq(CASES),
        "expected exactly the two unchanged pure parser tests",
    )?;
    Ok(body.into_bytes())
}
