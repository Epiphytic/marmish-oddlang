//! Replace only the recovered MissingRange overlay with the checked Rust role binding.
use crate::{
    api_composition::{Descriptor, checked_path, set_member},
    *,
};
use serde_json::json;

pub(crate) fn descriptors(root: &Path) -> Result<Vec<Descriptor>> {
    const ENGINE: &str = "marmish-engine";
    let mut workspace = load(&root.join("workspace-v7-canonical-recovered-range.json"))?;
    let mut package = load(&checked_path(
        root,
        &workspace["members"][ENGINE]["manifest"],
    )?)?;
    let donor = load(&root.join("engine-gap-range-native-idioms-package.json"))?;
    let [reason, range] = list(&donor["components"])?.as_slice() else {
        return Err("expected two role donor components".into());
    };
    let components = package["components"]
        .as_array_mut()
        .ok_or("missing engine components")?;
    let [.., previous_reason, previous_range] = components.as_mut_slice() else {
        return Err("missing recovered reason and range".into());
    };
    require(
        previous_range["reexports"] == json!(["MissingRange"]) && previous_range != range,
        "expected an explicit binding replacement for public MissingRange",
    )?;
    let mut expected = previous_range.clone();
    expected["overlay"] = pin(root, "engine-gap-range-native-idioms-v1/rust.json")?;
    require(
        previous_reason == reason && &expected == range,
        "role donor must change only the recovered range Rust binding",
    )?;
    *previous_range = expected;
    let package = Descriptor::new(
        "marmish-engine-canonical-range-native-idioms-package.json",
        &package,
    )?;
    set_member(&mut workspace, ENGINE, &package)?;
    Ok(vec![
        package,
        Descriptor::new(
            "workspace-v7-canonical-range-native-idioms.json",
            &workspace,
        )?,
    ])
}
