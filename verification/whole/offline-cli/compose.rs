//! Recover the auxiliary parser and its leading-option policy from the pinned Git source.
use crate::{
    api_composition::{Descriptor, checked_path, publish, set_member},
    cli_projection::pinned,
    *,
};
use clap::ValueEnum;
use serde_json::json;

#[derive(Clone, Copy, ValueEnum)]
pub enum Release {
    OfflineCli,
    TerminalPrefix,
}

const SOURCE: &str = "crates/marmish-native/src/rsync_cli.rs";
const SOURCE_SHA: &str = "4c40daed0c9cc0d11e198509729774d63bec240cebc8b6d9f0228ee911be2ad4";
const DIRECT_GAP: &str = "Direct rsh help and usage project the auxiliary declaration. Prefix scanning, separator policy and the successful auxiliary parse remain uncovered.";
const PREFIX_GAP: &str = "Leading root option policy applies to auxiliary terminal responses. Malformed prefixes, separators and successful auxiliary invocation are outside this projection.";

fn descriptor(path: String, value: &Value) -> Result<Descriptor> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(Descriptor::from_bytes(path, bytes))
}

pub fn compose(
    release: Release,
    root: &Path,
    compiler: &Path,
    original: &Path,
    write: bool,
) -> Result {
    let (domain, operation, parent, package_name, workspace_name) = match release {
        Release::OfflineCli => (
            "native-cli-terminal-v1",
            "cli-surface",
            "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json",
            "native-offline-cli-package.json",
            "workspace-v7-offline-cli.json",
        ),
        Release::TerminalPrefix => (
            "native-cli-prefix-v1",
            "cli-terminal-prefix",
            "workspace-v7-converged-diagnostics-cli.json",
            "native-terminal-prefix-package.json",
            "workspace-v7-terminal-prefix.json",
        ),
    };
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("parser.rs");
    let recovered = scratch.path().join("recovered.json");
    fs::write(&source, pinned(original, SOURCE, SOURCE_SHA)?)?;
    run(Command::new(compiler)
        .args(["recover", operation])
        .arg(&source)
        .arg(root.join(domain).join("selection.json"))
        .arg(&recovered))?;
    let recovered = load(&recovered)?;
    let mut workspace = load(&root.join(parent))?;
    let mut package = load(&checked_path(
        root,
        &workspace["members"]["marmish-native"]["manifest"],
    )?)?;
    let commands: Vec<_> = package["components"]
        .as_array_mut()
        .ok_or("missing components")?
        .iter_mut()
        .filter(|row| row["kind"] == "cli-surface")
        .collect();
    let [first, second] = commands.as_slice() else {
        return Err("expected two CLI roles".into());
    };
    require(
        first["graph"] == second["graph"],
        "parent CLI grammar differs by role",
    )?;
    let mut graph = load(&checked_path(root, &first["graph"])?)?;
    let gap = match release {
        Release::OfflineCli => {
            require(
                first["graph"]["path"] == "native-cli-v1/graph.json",
                "unexpected base grammar",
            )?;
            require(
                recovered["version"] == ""
                    && recovered.get("external").is_none_or(|value| value == false)
                    && recovered["root"].is_object()
                    && recovered["root"]
                        .get("subcommands")
                        .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty)),
                "unexpected auxiliary parser profile",
            )?;
            graph["terminal_subparsers"] = json!({"rsh":recovered["root"]});
            DIRECT_GAP
        }
        Release::TerminalPrefix => {
            graph["terminal_prefixes"] = json!({"rsh":recovered});
            let mut gaps = Vec::new();
            for gap in list(&graph["gaps"])? {
                let message = text(gap)?;
                if !message.starts_with("Direct rsh help and usage") {
                    gaps.push(gap.clone());
                }
            }
            graph["gaps"] = Value::Array(gaps);
            PREFIX_GAP
        }
    };
    graph["gaps"]
        .as_array_mut()
        .ok_or("missing grammar gaps")?
        .push(json!(gap));
    let graph = descriptor(format!("{domain}/graph.json"), &graph)?;
    let graph_pin = graph.pin();
    let mut outputs = vec![graph];
    for command in commands {
        command["graph"] = graph_pin.clone();
        if matches!(release, Release::OfflineCli) {
            let path = checked_path(root, &command["overlay"])?;
            let mut overlay = load(&path)?;
            let op = overlay["operations"]
                .as_object_mut()
                .and_then(|ops| ops.shift_remove("rsh"))
                .ok_or("missing auxiliary operation")?;
            require(
                overlay["handlers"].get(text(&op)?).is_none(),
                "auxiliary parser cannot replace a bound operation",
            )?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("invalid overlay name")?;
            let output = descriptor(format!("{domain}/{name}"), &overlay)?;
            command["overlay"] = output.pin();
            outputs.push(output);
        }
    }
    let package = descriptor(package_name.into(), &package)?;
    set_member(&mut workspace, "marmish-native", &package)?;
    outputs.push(package);
    outputs.push(descriptor(workspace_name.into(), &workspace)?);
    publish(root, outputs, !write)
}
