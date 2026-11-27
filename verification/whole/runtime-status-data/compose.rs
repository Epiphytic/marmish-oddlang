//! Extend the canonical native record views with the qualified runtime status overlay.
use crate::{
    api_composition::{Descriptor, canonical_member, set_member},
    *,
};
use std::path::Path;

pub(crate) fn descriptors(root: &Path) -> Result<Vec<Descriptor>> {
    let (mut workspace, mut package) = canonical_member(root, "marmish-native")?;
    let components = package["components"]
        .as_array_mut()
        .ok_or("missing native components")?;
    let mut candidates = components
        .iter_mut()
        .filter(|component| component["module"] == "record_views");
    let component = candidates.next().ok_or("missing record views")?;
    require(
        candidates.next().is_none() && component["kind"] == "public-records",
        "expected one public record view",
    )?;
    component["graph"] = pin(root, "native-journal-runtime-status-v1/graph.json")?;
    component["overlay"] = pin(root, "native-journal-runtime-status-v1/rust.json")?;
    let package = Descriptor::new("marmish-native-runtime-status-data-package.json", &package)?;
    set_member(&mut workspace, "marmish-native", &package)?;
    Ok(vec![
        package,
        Descriptor::new("workspace-v7-canonical-runtime-status.json", &workspace)?,
    ])
}
