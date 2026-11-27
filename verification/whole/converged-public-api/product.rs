//! Verify the prototype row against the constant release and all other rows against the error release.
use public_data_qualification::{Result, load, portable, product::check_members, require, sha};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub const ERRORS: &str = "workspace-v7-engine-public-error-identity";
const CONSTANT: &str = "workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit";
pub const RELEASE: &str = "workspace-v7-converged-public-api";
pub const MEMBER: &str = "marmish-prototype";

fn lock(release: &str) -> std::path::PathBuf {
    portable().join(release).join("workspace.lock")
}
pub fn parent_hashes() -> Result<Value> {
    Ok(json!({(ERRORS):sha(&lock(ERRORS))?, (CONSTANT):sha(&lock(CONSTANT))?}))
}
pub fn check(product: &Path) -> Result<Value> {
    let emitted = fs::read(product.join("workspace.lock"))?;
    require(
        emitted == fs::read(lock(RELEASE))?,
        "combined workspace differs from its pin",
    )?;
    let current: Value = serde_json::from_slice(&emitted)?;
    let members = current["members"]
        .as_object()
        .ok_or("missing generated members")?;
    for release in [ERRORS, CONSTANT] {
        let parent = load(&lock(release))?;
        let rows = parent["members"]
            .as_object()
            .ok_or("missing parent members")?;
        require(
            rows.len() == members.len()
                && members.keys().all(|k| rows.contains_key(k))
                && ["format", "cargoLockSha256", "workspaceCargoManifestSha256"]
                    .iter()
                    .all(|k| parent[k] == current[k]),
            format!("workspace metadata changed from {release}"),
        )?;
        for (member, row) in members {
            if (member == MEMBER) == (release == CONSTANT) {
                require(
                    row == &rows[member],
                    format!("{member} differs from qualified {release}"),
                )?;
            }
        }
    }
    check_members(product, &current, false)?;
    Ok(current)
}
