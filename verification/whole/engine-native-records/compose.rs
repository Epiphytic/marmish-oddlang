//! Extend the converged engine and native packages with their observed record groups.
use crate::{
    api_composition::{Descriptor, checked_path, set_member},
    *,
};
use serde_json::json;

pub(crate) fn descriptors(root: &Path) -> Result<Vec<Descriptor>> {
    let mut workspace = load(&root.join("workspace-v7-converged-public-api.json"))?;
    let mut outputs = Vec::new();
    for (member, domain, module, reexports, output) in [
        (
            "marmish-engine",
            "engine-scalar-records-v1",
            "model",
            json!(["InputReceipt", "MaintenanceReport"]),
            "engine-observational-records-package.json",
        ),
        (
            "marmish-native",
            "native-path-records-v1",
            "transport",
            json!(["ArtiDirs"]),
            "native-observational-records-package.json",
        ),
    ] {
        let mut package = load(&checked_path(
            root,
            &workspace["members"][member]["manifest"],
        )?)?;
        let mut component = json!({"kind":"public-records","module":module,"reexports":reexports,
            "graph":pin(root, &format!("{domain}/graph.json"))?,
            "overlay":pin(root, &format!("{domain}/rust.json"))?});
        if member == "marmish-native" {
            component["visibility"] = json!("public");
            component["rootReexports"] = json!([]);
        }
        package["components"]
            .as_array_mut()
            .ok_or("missing components")?
            .push(component);
        let descriptor = Descriptor::new(output, &package)?;
        set_member(&mut workspace, member, &descriptor)?;
        outputs.push(descriptor);
    }
    outputs.push(Descriptor::new(
        "workspace-v7-observational-records.json",
        &workspace,
    )?);
    Ok(outputs)
}
