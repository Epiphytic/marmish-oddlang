use super::*;
use serde_json::{Value, json};
use tempfile::tempdir;

fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn reference(portable: &Path, path: &Path) -> Result<Value> {
    Ok(json!({"path":path.strip_prefix(portable)?, "sha256":sha(&fs::read(path)?)}))
}

pub fn workspace(compiler: &Path, original: &Path, destination: &Path) -> Result<()> {
    if destination.try_exists()? {
        return Err("choose a new destination".into());
    }
    let revision = output(
        Command::new("git")
            .arg("-C")
            .arg(original)
            .args(["rev-parse", "HEAD"]),
    )?;
    if revision.trim() != PIN {
        return Err(format!("expected Marmish revision {PIN}").into());
    }
    if sha(&fs::read(
        original.join("crates/marmish-engine/src/policy.rs"),
    )?) != "bfaa35f2aa5ff29e48098f942c011a21ec66449c7c641d163b9cee0e6ff2fa2d"
    {
        return Err("policy engine differs from reviewed binding".into());
    }
    let portable = Path::new("oddlang/portable")
        .canonicalize()
        .map_err(|e| format!("run recover from the companion repository root: {e}"))?;
    let here = portable.join("recovered-command-policy-v1");
    let scratch = tempdir()?;
    let schema_path = scratch.path().join("schema.json");
    run(Command::new(compiler)
        .args(["recover", "restriction-lists"])
        .arg(original.join("crates/marmish-native/src/command_config.rs"))
        .arg(here.join("selection.json"))
        .arg(&schema_path))?;
    let schema = read(&schema_path)?;
    fs::copy(schema_path, here.join("schema.json"))?;
    let mut foreground = read(&portable.join("bounded-owner-prepare-v1/foreground.json"))?;
    foreground["room_prepare"]["restriction_lists"] =
        json!({"schema": schema, "decision":read(&here.join("decision.json"))?});
    let graph = here.join("foreground.json");
    write_json(&graph, &foreground)?;
    let mut package = read(&portable.join("marmish-native-retained-poll-package.json"))?;
    let mut owners = package["components"]
        .as_array_mut()
        .ok_or("components missing")?
        .iter_mut()
        .filter(|c| c["module"] == "foreground");
    let owner = owners.next().ok_or("foreground missing")?;
    if owners.next().is_some() {
        return Err("ambiguous foreground".into());
    }
    owner["graph"] = reference(&portable, &graph)?;
    let manifest = portable.join("marmish-native-command-policy-package.json");
    write_json(&manifest, &package)?;
    let mut workspace = read(&portable.join("workspace-retained-poll.json"))?;
    workspace["members"]["marmish-native"]["manifest"] = reference(&portable, &manifest)?;
    let release = portable.join("workspace-command-policy.json");
    write_json(&release, &workspace)?;
    run(Command::new(compiler)
        .args(["build", "native-workspace", "rust"])
        .arg(release)
        .arg(destination))?;
    fs::copy(
        destination.join("workspace.lock"),
        portable.join("workspace-command-policy/workspace.lock"),
    )?;
    Ok(())
}
