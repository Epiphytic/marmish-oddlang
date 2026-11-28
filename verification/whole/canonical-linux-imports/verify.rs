//! Compile the frozen public import corpus on native Linux ARM64 and compare the Darwin evidence.
mod role;
mod summary;
use clap::Parser;
use public_data_qualification::{
    Result,
    artifacts::source_pin,
    assembly::{COMPILER, DARWIN, LINUX, bytes_sha, resolved},
    imports::{canonical_libraries, canonical_profile, frozen_inventory},
    json_bytes, load,
    product::public_product_at,
    require, sha, stdout, text, whole,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    product: PathBuf,
    generated_target: PathBuf,
    inputs: PathBuf,
    original: PathBuf,
    original_target: PathBuf,
    receipt: PathBuf,
}

fn external_targets(
    product: &Path,
    original: &Path,
    generated: &Path,
    reference: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let generated = resolved(&std::path::absolute(generated)?)?;
    let reference = resolved(&std::path::absolute(reference)?)?;
    require(
        reference != generated,
        "the original and generated Cargo targets must differ",
    )?;
    for target in [&generated, &reference] {
        require(
            !target.starts_with(product) && !target.starts_with(original),
            "Cargo target inside an audited workspace",
        )?;
    }
    Ok((generated, reference))
}

fn report(
    workspace: &Value,
    source: Value,
    rustc: &str,
    cargo: &str,
    original: Value,
    generated: Value,
) -> Result<(Value, Value)> {
    let comparison = summary::compare(
        &json!({"originalLinux":original["libraries"],"generatedLinux":generated["libraries"],"generatedDarwin":canonical_libraries(DARWIN, true)?}),
    )?;
    let mut locks = json!({});
    for (name, member) in workspace["members"]
        .as_object()
        .ok_or("missing workspace members")?
    {
        locks[name] = text(&member["standalonePackageLockSha256"])?.into();
    }
    let darwin = whole().join("canonical-workspace/aarch64-apple-darwin.json");
    let mut receipt = json!({
        "format":"oddlang-canonical-linux-import-audit-v1","rustc":rustc.lines().collect::<Vec<_>>(),"cargo":cargo.lines().collect::<Vec<_>>(),
        "candidate":canonical_profile(),"candidateWorkspaceManifestSha256":workspace["inputSha256"],
        "candidateCargoLockSha256":workspace["cargoLockSha256"],"candidatePackageLocks":locks,
        "original":source,"darwinEvidenceSha256":sha(&darwin)?,"darwinInventoryProvenanceSha256":sha(&whole().join("public-types/provenance.json"))?,
        "verifierSha256":bytes_sha(include_bytes!("verify.rs")),"sharedVerifierSha256":bytes_sha(include_bytes!("role.rs")),
        "originalBuild":original,"generatedBuild":generated
    });
    for (key, value) in comparison.as_object().ok_or("missing import summary")? {
        receipt[key] = value.clone();
    }
    Ok((receipt, comparison))
}

fn main() -> Result {
    let a = Args::parse();
    let version = |name| stdout(Command::new(name).args(["+1.97.1", "-vV"]));
    let rustc = version("rustc")?;
    require(
        rustc.lines().any(|s| s == format!("host: {LINUX}"))
            && rustc
                .lines()
                .any(|s| s == format!("commit-hash: {COMPILER}")),
        "expected pinned native Linux ARM64 compiler",
    )?;
    let product = a.product.canonicalize()?;
    let original = a.original.canonicalize()?;
    let inputs = a.inputs.canonicalize()?;
    let (generated_target, original_target) =
        external_targets(&product, &original, &a.generated_target, &a.original_target)?;
    let profile = canonical_profile();
    let manifest = || {
        public_product_at(
            &product,
            &inputs,
            text(&profile["release"])?,
            text(&profile["input_revision"])?,
            text(&profile["lock_sha256"])?,
        )
    };
    let source = source_pin(&original)?;
    frozen_inventory(&inputs)?;
    let before = manifest()?;
    let observe = |root: &Path, target: &Path, generated| {
        role::report(
            &role::build(root, target, LINUX)?,
            root,
            target,
            &original,
            LINUX,
            generated,
        )
    };
    let upstream = observe(&original, &original_target, false)?;
    let candidate = observe(&product, &generated_target, true)?;
    require(
        source == source_pin(&original)? && before == manifest()?,
        "source or generated product changed during the import audit",
    )?;
    frozen_inventory(&inputs)?;
    let (receipt, comparison) = report(
        &load(&product.join("workspace.lock"))?,
        source,
        &rustc,
        &version("cargo")?,
        upstream,
        candidate,
    )?;
    fs::write(a.receipt, json_bytes(&receipt)?)?;
    println!("{}", serde_json::to_string_pretty(&comparison)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconstructs_published_receipt_from_the_role_observations() -> Result {
        let archive =
            load(&whole().join("canonical-linux-imports/aarch64-unknown-linux-gnu.json"))?;
        let lock = load(
            &public_data_qualification::portable()
                .join("workspace-v7-canonical-offline/workspace.lock"),
        )?;
        let lines = |v: &Value| -> Result<String> {
            Ok(public_data_qualification::list(v)?
                .iter()
                .map(text)
                .collect::<Result<Vec<_>>>()?
                .join("\n"))
        };
        let (mut rebuilt, _) = report(
            &lock,
            archive["original"].clone(),
            &lines(&archive["rustc"])?,
            &lines(&archive["cargo"])?,
            archive["originalBuild"].clone(),
            archive["generatedBuild"].clone(),
        )?;
        for key in ["verifierSha256", "sharedVerifierSha256"] {
            assert_ne!(rebuilt[key], archive[key]);
            rebuilt[key] = archive[key].clone();
        }
        assert_eq!(rebuilt, archive);
        Ok(())
    }
    #[test]
    fn prevents_target_overlap_through_an_existing_alias() -> Result {
        let dir = tempfile::tempdir()?;
        let base = dir.path().canonicalize()?;
        let product = base.join("product");
        let original = base.join("original");
        fs::create_dir(&product)?;
        fs::create_dir(&original)?;
        let alias = base.join("alias");
        std::os::unix::fs::symlink(&product, &alias)?;
        let target = base.join("new-target");
        assert!(external_targets(&product, &original, &target, &target).is_err());
        assert!(external_targets(&product, &original, &alias.join("new"), &target).is_err());
        assert!(
            external_targets(&product, &original, &target, &base.join("new-reference")).is_ok()
        );
        Ok(())
    }
}
