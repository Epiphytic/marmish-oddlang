//! Compare declared native vault values and independently changed UTF-8 bounds.
use clap::{Parser, Subcommand};
use copy_checkpoint_replay::{build_compiler, stage};
use public_data_qualification::{
    assembly::{self, ORIGINAL, Sandbox},
    vault_witness as witness, *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FRAMEWORK: &str = "c579dcdfa8f0e94d3dc894242908475552cdbd83";
const SCOPE: [&str; 6] = [
    "Error",
    "KeyRef",
    "Label",
    "Metadata",
    "Namespace",
    "Purpose",
];
const PACKAGE: &str = "vault-value-domain-package.json";
const MEMBER: &str = "crates/marmish-vault";
const SOURCE: &str = "crates/marmish-vault/src/lib.rs";

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Qualify {
        original: PathBuf,
        framework: PathBuf,
        receipt: PathBuf,
        #[arg(long)]
        rust_capture: PathBuf,
    },
    Workspace {
        original: PathBuf,
        product: PathBuf,
        work: PathBuf,
    },
}

fn native_type(t: &Value) -> Result<String> {
    if t["kind"] == "unsigned-integer" {
        return Ok(format!(
            "u{}",
            t["bits"].as_u64().ok_or("missing integer width")?
        ));
    }
    require(
        t["kind"] == "named" && t["arguments"] == json!([]),
        "expected simple named field",
    )?;
    let id = text(&t["id"])?;
    Ok(id
        .trim_end_matches("::{type}")
        .rsplit("::")
        .next()
        .unwrap()
        .into())
}

fn ordered(fields: &Value) -> Result<Vec<&Value>> {
    let mut pairs = list(fields)?
        .iter()
        .map(|f| Ok((f["position"].as_u64().ok_or("missing position")?, f)))
        .collect::<Result<Vec<_>>>()?;
    pairs.sort_by_key(|(n, _)| *n);
    require(
        pairs.windows(2).all(|p| p[0].0 != p[1].0),
        "duplicate declaration position",
    )?;
    Ok(pairs.into_iter().map(|(_, v)| v).collect())
}

fn shapes(raw: &[u8]) -> Result<Value> {
    let capture: Value = serde_json::from_slice(raw)?;
    require(
        capture["crateName"] == "marmish_vault",
        "wrong inventory crate",
    )?;
    let mut out = json!({});
    for d in list(&capture["graph"]["declarations"])? {
        let name = text(&d["name"])?;
        if !SCOPE.contains(&name) || text(&d["id"])?.contains("::helper::") {
            continue;
        }
        require(
            out.get(name).is_none() && d["parameters"] == json!([]),
            "duplicate or parameterized public declaration",
        )?;
        out[name] = match text(&d["kind"])? {
            "sum" => {
                let mut variants = vec![];
                for v in ordered(&d["variants"])? {
                    require(
                        v["form"] == "unit" && v["fields"] == json!([]),
                        "expected a fieldless variant",
                    )?;
                    variants.push(text(&v["name"])?);
                }
                json!({"kind":"sum","orderedVariants":variants})
            }
            "product" => {
                let fields = ordered(&d["fields"])?
                    .into_iter()
                    .map(|f| Ok(json!({"name":f["name"],"type":native_type(&f["type"])?})))
                    .collect::<Result<Vec<_>>>()?;
                json!({"kind":"product","form":d["form"],"orderedFields":fields})
            }
            _ => return Err("unsupported vault declaration".into()),
        };
    }
    require(
        SCOPE.iter().all(|k| out.get(k).is_some()),
        "missing public vault declaration",
    )?;
    Ok(out)
}

fn consumer(member: &Path, lock: &Path, work: &Path, expected: &[String], privacy: bool) -> Result {
    witness::reference(
        member,
        lock,
        work,
        "observe-vault-values",
        &whole().join("vault-values/caller.rs"),
        false,
    )?;
    let cargo = || {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(work)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET")
            .arg("+1.97.1");
        cmd
    };
    run(cargo().args(["build", "--release", "--locked", "--offline"]))?;
    let observed = stdout(&mut Command::new(
        work.join("target/release/observe-vault-values"),
    ))?;
    require(
        observed.lines().eq(expected.iter().map(String::as_str)),
        format!("vault observer differs for {}", member.display()),
    )?;
    if privacy {
        for (ty, expr, code) in [
            (
                "KeyRef",
                "KeyRef { version: 2, purpose: Purpose::Nostr, id: \"arbitrary\".into() }",
                "E0451",
            ),
            ("Label", "Label(\"arbitrary\".into())", "E0423"),
        ] {
            fs::write(
                work.join("src/main.rs"),
                format!(
                    "use marmish_vault::{{KeyRef, Label, Purpose}};\nfn main() {{ let _ = {expr}; }}\n"
                ),
            )?;
            let probe = capture(
                cargo().args(["check", "--release", "--locked", "--offline"]),
                600,
                None,
            )?;
            let err = std::str::from_utf8(&probe.stderr)?;
            require(
                !probe.status.success() && [code, ty, "private"].iter().all(|s| err.contains(s)),
                format!("missing private storage diagnostic for {ty}"),
            )?;
        }
    }
    Ok(())
}

fn qualify(original: &Path, framework: &Path, tool: &Path) -> Result<Value> {
    let original = original.canonicalize()?;
    let framework = framework.canonicalize()?;
    assembly::checkout(&original, ORIGINAL, false)?;
    // The historical qualifier read only the pinned framework tree.
    let store = tempfile::tempdir()?;
    let private = store.path().canonicalize()?;
    let fw = private.join("framework");
    let head = stdout(
        Command::new("git")
            .current_dir(&framework)
            .args(["rev-parse", "HEAD"]),
    )?;
    require(head.trim() == FRAMEWORK, "wrong framework revision")?;
    assembly::archive(&framework, FRAMEWORK, &fw)?;
    let tool = tool.canonicalize()?;
    let data = portable().join("vault-values-v1");
    let source_sha = sha(&original.join(SOURCE))?;
    require(
        load(&data.join("provenance.json"))?["sourceSha256"] == source_sha,
        "source provenance differs",
    )?;
    let mutant = private.join("mutant");
    assembly::archive(&original, ORIGINAL, &mutant)?;
    let path = mutant.join(SOURCE);
    let before = fs::read_to_string(&path)?;
    let bound = "|| value.len() > 128";
    require(
        before.matches(bound).count() == 1,
        "expected one source label bound",
    )?;
    fs::write(&path, before.replace(bound, "|| value.len() > 127"))?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let host = rust
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let cli = build_compiler(&fw, &private.join("tools"), "+1.97.1")?;
    let temp = tempfile::tempdir()?;
    let work = temp.path().canonicalize()?;
    let compiler = work.join("oddlang");
    fs::copy(cli, &compiler)?;
    let (plan, mut manifest) = stage(&work.join("inputs"), PACKAGE, false)?;
    let providers = work.join("providers");
    fs::create_dir(&providers)?;
    let companion = assembly::root().canonicalize()?;
    let sandbox = Sandbox::new(
        host,
        &work,
        &[&companion, &original, &framework, &private],
        &providers,
    )?;
    let caller = whole().join("vault-values/caller.rs");
    let shape_file = data.join("shapes.json");
    let project = |name: &str| -> Result<PathBuf> {
        let dest = work.join(name);
        sandbox.build_native_package(
            &compiler,
            &plan,
            &dest,
            &[
                original.join(SOURCE),
                mutant.join(SOURCE),
                caller.clone(),
                shape_file.clone(),
                fw.join("Cargo.toml"),
            ],
        )?;
        Ok(dest)
    };
    let native = project("native")?;
    require(
        fs::read(native.join("package.lock"))? == fs::read(data.join("package.lock"))?,
        "baseline package lock differs",
    )?;
    let expected_shapes = load(&shape_file)?;
    for root in [original.join(MEMBER), native.clone()] {
        let raw = witness::capture_shapes(&tool, &root, host)?;
        require(
            shapes(&raw)? == expected_shapes,
            "native vault shapes differ",
        )?;
    }
    let components = manifest["components"]
        .as_array_mut()
        .ok_or("missing components")?;
    let mut checked = components
        .iter_mut()
        .filter(|c| c["kind"] == "checked-values");
    let component = checked.next().ok_or("missing checked-values")?;
    require(
        checked.next().is_none(),
        "ambiguous checked-values component",
    )?;
    let pin = &mut component["graph"];
    let graph_path = plan.parent().unwrap().join(text(&pin["path"])?);
    let original_graph = sha(&graph_path)?;
    let mut graph = load(&graph_path)?;
    let mut matches = graph["types"]["urn:marmish:vault:label"]["guards"]
        .as_array_mut()
        .ok_or("missing guards")?
        .iter_mut()
        .filter(|r| r["kind"] == "utf8-byte-length");
    let rule = matches.next().ok_or("missing byte limit")?;
    require(
        matches.next().is_none() && rule["maximum"] == 128,
        "unexpected label limit",
    )?;
    rule["maximum"] = json!(127);
    fs::write(&graph_path, json_bytes(&graph)?)?;
    pin["sha256"] = json!(sha(&graph_path)?);
    fs::write(&plan, json_bytes(&manifest)?)?;
    let changed = project("changed")?;
    let baseline = witness::expected(128);
    let altered = witness::expected(127);
    let different = baseline
        .iter()
        .zip(&altered)
        .filter(|(a, b)| a != b)
        .count();
    require(
        baseline.len() == altered.len() && different > 0,
        "mutation did not change expectations",
    )?;
    for (name, member, lock, want, privacy) in [
        (
            "upstream",
            original.join(MEMBER),
            original.join("Cargo.lock"),
            &baseline,
            true,
        ),
        (
            "upstream-mutated",
            mutant.join(MEMBER),
            mutant.join("Cargo.lock"),
            &altered,
            false,
        ),
        (
            "generated",
            native.clone(),
            native.join("Cargo.lock"),
            &baseline,
            true,
        ),
        (
            "generated-mutated",
            changed.clone(),
            changed.join("Cargo.lock"),
            &altered,
            false,
        ),
    ] {
        consumer(&member, &lock, &private.join(name), want, privacy)?;
    }
    Ok(
        json!({"original":ORIGINAL,"framework":FRAMEWORK,"rust":rust.trim(),"declaredShapes":SCOPE,"observations":4*baseline.len(),"changedOutcomes":different,"originalSourceSha256":source_sha,"mutatedSourceSha256":sha(&path)?,"originalGraphSha256":original_graph,"mutatedGraphSha256":sha(&graph_path)?,"originalPackageLockSha256":sha(&native.join("package.lock"))?,"mutatedPackageLockSha256":sha(&changed.join("package.lock"))?,"callerSha256":sha(&caller)?,"shapesSha256":sha(&shape_file)?,"sourceReadDenied":true,"privateStorageRejectsExternalConstruction":true}),
    )
}

fn main() -> Result {
    match Args::parse().mode {
        Mode::Qualify {
            original,
            framework,
            receipt,
            rust_capture,
        } => fs::write(
            receipt,
            json_bytes(&qualify(&original, &framework, &rust_capture)?)?,
        )?,
        Mode::Workspace {
            original,
            product,
            work,
        } => {
            let original = original.canonicalize()?;
            let product = product.canonicalize()?;
            assembly::checkout(&original, ORIGINAL, false)?;
            let baseline = witness::expected(128);
            for (label, member, lock) in [
                (
                    "original-vault",
                    original.join(MEMBER),
                    original.join("Cargo.lock"),
                ),
                (
                    "generated-vault",
                    product.join("marmish-vault"),
                    product.join("Cargo.lock"),
                ),
            ] {
                consumer(&member, &lock, &work.join(label), &baseline, true)?;
            }
            println!(
                "{}",
                json!({"vaultRows":baseline.len(),"vaultStdoutSha256":assembly::bytes_sha(format!("{}\n",baseline.join("\n")).as_bytes())})
            );
        }
    }
    Ok(())
}
