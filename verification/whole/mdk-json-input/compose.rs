//! Compose the standalone bounded parser into the canonical MDK test workspace.
use crate::{
    api_composition::{Descriptor, canonical_member, checked_path, set_member},
    *,
};
use serde_json::json;
use std::path::Path;

const MEMBER: &str = "marmish-mdk";

fn add_dev_edge(lock: &str) -> Result<String> {
    let mut expected: toml::Table = toml::from_str(lock)?;
    let packages = expected
        .get_mut("package")
        .and_then(toml::Value::as_array_mut)
        .ok_or("missing Cargo packages")?;
    let mut rows = packages
        .iter_mut()
        .filter(|row| row.get("name").and_then(toml::Value::as_str) == Some(MEMBER));
    let row = rows
        .next()
        .and_then(toml::Value::as_table_mut)
        .ok_or("missing MDK lock row")?;
    require(
        rows.next().is_none()
            && row.len() == 2
            && row.get("version").and_then(toml::Value::as_str) == Some("0.1.0"),
        "expected one dependency-free MDK lock row",
    )?;
    row.insert(
        "dependencies".into(),
        toml::Value::Array(vec!["serde_json".into()]),
    );
    let marker = "name = \"marmish-mdk\"\nversion = \"0.1.0\"\n";
    require(
        lock.matches(marker).count() == 1,
        "ambiguous canonical MDK lock row",
    )?;
    let edited = lock.replace(
        marker,
        &format!("{marker}dependencies = [\n \"serde_json\",\n]\n"),
    );
    require(
        toml::from_str::<toml::Table>(&edited)? == expected,
        "composition changed another Cargo entry",
    )?;
    Ok(edited)
}

pub(crate) fn descriptors(root: &Path) -> Result<Vec<Descriptor>> {
    let (mut workspace, mut package) = canonical_member(root, MEMBER)?;
    let bounded = load(&root.join("mdk-json-input-package.json"))?;
    let (parser, prior) = list(&bounded["components"])?
        .split_last()
        .ok_or("missing standalone parser")?;
    require(
        prior == list(&package["components"])?
            && parser["kind"] == "bounded-json-input"
            && parser["reexports"] == json!([])
            && bounded["dependencies"] == json!({"serde_json":{"version":"1.0.151"}}),
        "standalone MDK parser differs from canonical member",
    )?;
    for descriptor in [&parser["graph"], &parser["overlay"], &bounded["cargoLock"]] {
        checked_path(root, descriptor)?;
    }
    package["components"] = bounded["components"].clone();
    package["cargoLock"] = bounded["cargoLock"].clone();
    package["devDependencies"] = bounded["dependencies"].clone();
    package.sort_all_objects();
    let package = Descriptor::new("mdk-json-test-package.json", &package)?;
    set_member(&mut workspace, MEMBER, &package)?;
    let lock = fs::read_to_string(checked_path(root, &workspace["cargoLock"])?)?;
    let lock = Descriptor::from_bytes(
        "workspace-v7-canonical-mdk-test-seam/Cargo.lock",
        add_dev_edge(&lock)?.into_bytes(),
    );
    workspace["cargoLock"] = lock.pin();
    workspace.sort_all_objects();
    Ok(vec![
        package,
        lock,
        Descriptor::new("workspace-v7-canonical-mdk-test-seam.json", &workspace)?,
    ])
}
