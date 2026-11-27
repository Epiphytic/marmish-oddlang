//! Compare the compiled public error, identity, constant and buffered-reader roles.
use crate::{
    manifest_native::{ParserRun, artifacts, depend, external, one, parent, reference},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
struct Library<'a> {
    name: &'a str,
    lib: &'a Path,
    deps: &'a Path,
    edition: &'a str,
}
impl Library<'_> {
    fn compiler(&self, serde: &Path) -> Result<Command> {
        let mut c = Command::new("rustc");
        c.args(["+1.97.1", self.edition]);
        depend(&mut c, self.deps);
        depend(&mut c, parent(serde)?);
        external(&mut c, self.name, self.lib);
        external(&mut c, "serde", serde);
        Ok(c)
    }
    fn choose_serde(&self, rows: &[Value], scratch: &Path) -> Result<PathBuf> {
        let source = scratch.join("trait_identity.rs");
        fs::write(
            &source,
            "fn require<T: serde::Serialize>() {}\nfn main() { require::<marmish_native::journal::Conversation>(); }\n",
        )?;
        let mut found = Vec::new();
        for p in artifacts(rows, "serde", "rlib")? {
            if capture(
                self.compiler(&p)?
                    .arg("--emit=metadata")
                    .arg(&source)
                    .arg("-o")
                    .arg(scratch.join("trait.rmeta")),
                90,
                None,
            )?
            .status
            .success()
            {
                found.push(p);
            }
        }
        let [candidate] = found
            .try_into()
            .map_err(|_| "expected one compatible native Serde trait")?;
        Ok(candidate)
    }
    fn observe(&self, serde: &Path, lane: &str, scratch: &Path) -> Result<Value> {
        fs::create_dir(scratch)?;
        let exe = scratch.join("probe");
        run(self
            .compiler(serde)?
            .arg(whole().join(lane).join("probe.rs"))
            .arg("-o")
            .arg(&exe))?;
        let output = stdout(&mut Command::new(exe))?;
        let controls: Value = serde_json::from_str(include_str!("compile-controls.json"))?;
        let mut verified = json!({});
        let src = scratch.join("negative.rs");
        for (name, row) in controls[lane]
            .as_object()
            .ok_or("missing compiler controls")?
        {
            fs::write(&src, text(&row["source"])?)?;
            let r = capture(
                self.compiler(serde)?
                    .arg("--error-format=json")
                    .arg(&src)
                    .arg("-o")
                    .arg(scratch.join("negative")),
                90,
                None,
            )?;
            let mut codes = BTreeSet::new();
            for line in std::str::from_utf8(&r.stderr)?.lines() {
                let v: Value = serde_json::from_str(line)?;
                if (lane == "backend-choice" || v["level"] == "error")
                    && let Some(code) = v["code"]["code"].as_str()
                {
                    codes.insert(code.to_owned());
                }
            }
            require(
                !r.status.success() && codes == BTreeSet::from([text(&row["code"])?.to_owned()]),
                format!("unexpected {lane} diagnostic for {name}: {codes:?}"),
            )?;
            verified[name] = row["code"].clone();
        }
        Ok(json!({"output":output,"negativeRustDiagnostics":verified}))
    }
}
impl ParserRun<'_> {
    pub(crate) fn native_serde(&self, rows: &[Value], deps: &Path) -> Result<PathBuf> {
        Library {
            name: "marmish_native",
            lib: &one(rows, "marmish_native")?,
            deps,
            edition: "--edition=2024",
        }
        .choose_serde(rows, self.scratch)
    }
    pub fn public_errors(&self, work: &Path, cli: &Path) -> Result<Value> {
        let recovered = self.scratch.join("engine-recovered");
        run(Command::new(cli)
            .args(["recover", "finite-error-domain"])
            .arg(self.original.join("crates/marmish-engine/src/model.rs"))
            .arg(whole().join("engine-public-error/selection.json"))
            .arg(&recovered))?;
        for name in ["graph.json", "rust.json"] {
            require(
                fs::read(recovered.join(name))?
                    == fs::read(work.join("inputs/engine-public-error-v1").join(name))?,
                "recovered engine types differ from staged declarations",
            )?;
        }
        let deps = self.product.join("target/release/deps");
        let mut report = json!({});
        for (lane, package, name, edition, role) in [
            (
                "engine-public-error",
                "marmish-engine",
                "marmish_engine",
                "--edition=2021",
                "enginePublicError",
            ),
            (
                "backend-choice",
                "marmish-native",
                "marmish_native",
                "--edition=2024",
                "identity",
            ),
        ] {
            let upstream = reference(self.original, self.original_target, package)?;
            let old = one(&upstream, name)?;
            let new = one(self.build, name)?;
            let olddeps = parent(&old)?.join("deps");
            let mut roles = Vec::new();
            for (tag, p, dependencies, rows) in [
                ("original", &old, &olddeps, &upstream[..]),
                ("generated", &new, &deps, self.build),
            ] {
                let lib = Library {
                    name,
                    lib: p,
                    deps: dependencies,
                    edition,
                };
                let serde = if lane == "backend-choice" {
                    lib.choose_serde(rows, self.scratch)?
                } else {
                    one(&upstream, "serde")?
                };
                roles.push(lib.observe(
                    &serde,
                    lane,
                    &self.scratch.join(format!("{lane}-{tag}")),
                )?);
            }
            require(
                roles[0] == roles[1],
                format!("{lane} native observations differ"),
            )?;
            let mut v = roles.remove(0);
            if role == "identity" {
                v["scope"] = "marmish_native::identity::BackendChoice".into();
            } else {
                let count = text(&v["output"])?
                    .lines()
                    .filter_map(|l| l.strip_prefix("error|")?.split('|').next())
                    .collect::<BTreeSet<_>>()
                    .len();
                require(count == 26, "expected all 26 engine errors")?;
                v["scope"] = json!([
                    "EngineError",
                    "NotFoundReason",
                    "NotExecutableReason",
                    "ResolutionReason",
                    "Result"
                ]);
                v["originalLibrarySha256"] = sha(&old)?.into();
                v["sealedLibrarySha256"] = sha(&new)?.into();
                v["recovery"] = load(&recovered.join("observation.json"))?;
                v["distinctVariants"] = count.into();
            }
            report[role] = v;
        }
        Ok(report)
    }
    pub fn inherited(&self, work: &Path, cli: &Path) -> Result<Value> {
        let mut report = self.public_errors(work, cli)?;
        let prior = self.scratch.join("prior");
        run(Command::new(cli)
            .args(["build", "native-package", "rust"])
            .arg(portable().join("prototype-buffer-package.json"))
            .arg(&prior))?;
        let leaf = "src/group_0/__projection_0.rs";
        require(
            sha(&prior.join(leaf))? == sha(&self.product.join("marmish-prototype").join(leaf))?,
            "private reader changed in combined workspace",
        )?;
        report["bufferedInput"] =
            buffered::observe(self.product, self.original, self.host, Some(&prior))?;
        let upstream = reference(self.original, self.original_target, "marmish-prototype")?;
        for (label, rows) in [
            ("original-constant", &upstream[..]),
            ("generated-constant", self.build),
        ] {
            let lib = one(rows, "marmish_prototype")?;
            report["publicConstant"][label] =
                prototype_constant::caller(&lib, &self.scratch.join(label))?;
        }
        Ok(report)
    }
}
