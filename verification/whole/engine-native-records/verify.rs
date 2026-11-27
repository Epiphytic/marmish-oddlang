//! Compare the engine and native public type releases with their immediate predecessors.
use crate::{
    engine_caller::{self, Role},
    manifest_native::{ParserRun, one, parent, reference},
    *,
};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Release {
    Errors,
    Records,
}

impl Release {
    pub fn profile(
        self,
    ) -> (
        &'static str,
        &'static str,
        &'static str,
        api_composition::Release,
    ) {
        match self {
            Self::Errors => (
                "workspace-v7-engine-public-error-identity",
                buffered::RELEASE,
                "marmish-seven-member-typed-error-identity-v1",
                api_composition::Release::EnginePublicError,
            ),
            Self::Records => (
                "workspace-v7-observational-records",
                "workspace-v7-converged-public-api",
                "marmish-converged-observational-records-v1",
                api_composition::Release::ObservationalRecords,
            ),
        }
    }
    pub fn check(self, product: &Path) -> Result<Value> {
        let (release, base, _, _) = self.profile();
        let pinned = |name: &str| portable().join(name).join("workspace.lock");
        let actual = fs::read(product.join("workspace.lock"))?;
        require(
            actual == fs::read(pinned(release))?,
            "public type workspace lock drift",
        )?;
        let current: Value = serde_json::from_slice(&actual)?;
        let previous = load(&pinned(base))?;
        require(
            current["cargoLockSha256"] == previous["cargoLockSha256"],
            "dependency lock changed",
        )?;
        if self == Self::Records {
            require(
                current["workspaceCargoManifestSha256"] == previous["workspaceCargoManifestSha256"],
                "workspace manifest changed",
            )?;
        }
        let new = current["members"]
            .as_object()
            .ok_or("missing member table")?;
        let old = previous["members"]
            .as_object()
            .ok_or("missing predecessor member table")?;
        require(
            new.len() == 7 && new.len() == old.len(),
            "unexpected member count",
        )?;
        for (name, row) in new {
            let prior = old.get(name).ok_or("member absent from predecessor")?;
            require(
                (row != prior) == matches!(name.as_str(), "marmish-engine" | "marmish-native")
                    && row["dependencies"] == prior["dependencies"],
                format!("unexpected change to member {name}"),
            )?;
        }
        product::check_members(product, &current, false)?;
        Ok(current)
    }
}
impl ParserRun<'_> {
    pub fn records(&self, work: &Path, cli: &Path) -> Result<Value> {
        let upstream = reference(self.original, self.original_target, "marmish-native")?;
        let old_deps = parent(&one(&upstream, "marmish_native")?)?.join("deps");
        let new_deps = self.product.join("target/release/deps");
        let old_serde = self.native_serde(&upstream, &old_deps)?;
        let new_serde = self.native_serde(self.build, &new_deps)?;
        let mut report = json!({});
        for (label, crate_name, role, domain, source, scope) in [
            (
                "engine",
                "marmish_engine",
                Role::Records,
                "engine-scalar-records-v1",
                "crates/marmish-engine/src/model.rs",
                vec![
                    "marmish_engine::InputReceipt",
                    "marmish_engine::MaintenanceReport",
                ],
            ),
            (
                "native",
                "marmish_native",
                Role::Native,
                "native-path-records-v1",
                "crates/marmish-native/src/transport.rs",
                vec!["marmish_native::transport::ArtiDirs"],
            ),
        ] {
            let recovered = self.scratch.join(format!("{label}-record-recovered"));
            run(Command::new(cli)
                .args(["recover", "public-root-records"])
                .arg(self.original.join(source))
                .arg(whole().join(format!("engine-native-records/{label}-selection.json")))
                .arg(&recovered))?;
            for file in ["graph.json", "rust.json"] {
                require(
                    fs::read(recovered.join(file))?
                        == fs::read(work.join("inputs").join(domain).join(file))?,
                    format!("recovered {label} {file} differs"),
                )?;
            }
            let old = one(&upstream, crate_name)?;
            let new = one(self.build, crate_name)?;
            let baseline = engine_caller::observe(
                role,
                &old,
                &old_deps,
                &old_serde,
                &self.scratch.join(format!("{label}-record-original")),
            )?;
            let observed = engine_caller::observe(
                role,
                &new,
                &new_deps,
                &new_serde,
                &self.scratch.join(format!("{label}-record-generated")),
            )?;
            require(
                baseline == observed,
                format!("sealed {label} records differ from original"),
            )?;
            report[label] = json!({"scope":scope,"output":observed.0,"negativeRustDiagnostics":observed.1,
                "recovery":load(&recovered.join("recovery.lock"))?,"originalLibrarySha256":sha(&old)?,"generatedLibrarySha256":sha(&new)?});
        }
        Ok(report)
    }
}
