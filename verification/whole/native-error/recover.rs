//! Recover the reviewed native error model and one controlled change of category.
use super::native_probe;
use public_data_qualification::{
    assembly::{ORIGINAL, bytes_sha, checkout, stage},
    *,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SOURCE: &str = "crates/marmish-native/src/error.rs";
const SPEC: &str = "workspace-v6-native-error.json";
const GRAPH: &str = "native-error-v1/graph.json";
const OVERLAY: &str = "native-error-v1/rust.json";

fn write(path: &Path, value: &Value) -> Result {
    fs::write(path, json_bytes(value)?)?;
    Ok(())
}

fn mutate_source(original: &Path, mutant: &Path) -> Result {
    run(Command::new("git")
        .args(["clone", "--quiet", "--shared", "--no-checkout"])
        .arg(original)
        .arg(mutant))?;
    run(Command::new("git")
        .current_dir(mutant)
        .args(["checkout", "--quiet", "--detach", ORIGINAL]))?;
    let path = mutant.join(SOURCE);
    let src = fs::read_to_string(&path)?;
    let old = r#"Quota => "capacity""#;
    require(src.matches(old).count() == 1, "expected one quota category")?;
    fs::write(&path, src.replace(old, r#"Quota => "transfer_limit""#))?;
    let mut commit = Command::new("git");
    for role in ["AUTHOR", "COMMITTER"] {
        commit
            .env(format!("GIT_{role}_NAME"), "Qualification Fixture")
            .env(format!("GIT_{role}_EMAIL"), "fixture@example.invalid")
            .env(format!("GIT_{role}_DATE"), "2026-11-26T12:00:00Z");
    }
    run(commit.current_dir(mutant).args([
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--only",
        "-m",
        "Change the copy quota category",
        "--",
        SOURCE,
    ]))?;
    require(
        stdout(
            Command::new("git")
                .current_dir(mutant)
                .args(["diff", "--name-only", ORIGINAL]),
        )?
        .lines()
        .eq([SOURCE]),
        "unexpected files in mutation",
    )
}

fn compose(inputs: &Path, recovered: &Path) -> Result<PathBuf> {
    let base = portable();
    stage(&base.join(SPEC), inputs)?;
    fs::copy(recovered.join("graph.json"), inputs.join(GRAPH))?;
    fs::copy(recovered.join("rust.json"), inputs.join(OVERLAY))?;
    let mut native = load(&base.join("native-configured-lifecycle-package.json"))?;
    native["components"]
        .as_array_mut()
        .ok_or("missing components")?
        .push(json!({
            "kind":"finite-error-domain",
            "module":"error",
            "graph":pin(inputs,GRAPH)?,
            "overlay":pin(inputs,OVERLAY)?,
            "visibility":"public",
            "reexports":["Error","Result"]
        }));
    let member = inputs.join("native-error-package.json");
    write(&member, &native)?;
    let mut ws = load(&base.join("workspace-v6.json"))?;
    let lock = fs::read_to_string(base.join(text(&ws["cargoLock"]["path"])?))?;
    let marker = "name = \"marmish-native\"\n";
    let (prefix, rest) = lock
        .split_once(marker)
        .ok_or("missing native in Cargo lock")?;
    let (section, suffix) = rest
        .split_once("\n[[package]]")
        .ok_or("incomplete Cargo lock")?;
    let libc = " \"libc\",\n";
    require(
        section.matches(libc).count() == 1 && !section.contains("\"marmish-stream\""),
        "unexpected v6 native dependencies",
    )?;
    let changed = section.replace(libc, " \"libc\",\n \"marmish-stream\",\n");
    let output = inputs.join("native-error-v1/Cargo.lock");
    fs::write(
        &output,
        format!("{prefix}{marker}{changed}\n[[package]]{suffix}"),
    )?;
    ws["cargoLock"] = pin(inputs, "native-error-v1/Cargo.lock")?;
    ws["members"]["marmish-native"] = json!({"manifest":pin(inputs,"native-error-package.json")?,"dependencies":["marmish-stream"]});
    let manifest = inputs.join(SPEC);
    write(&manifest, &ws)?;
    Ok(manifest)
}

fn check_delta(base: &Path, mutant: &Path) -> Result {
    for name in ["graph.json", "rust.json", "observation.json"] {
        require(
            fs::read(base.join(name))? == fs::read(portable().join("native-error-v1").join(name))?,
            format!("baseline {name} differs from release"),
        )?;
    }
    require(
        fs::read(base.join("rust.json"))? == fs::read(mutant.join("rust.json"))?,
        "mutation changed the overlay",
    )?;
    let mut before = load(&base.join("graph.json"))?;
    let after = load(&mutant.join("graph.json"))?;
    let mut count = 0;
    for (table, entries) in before["categories"]
        .as_object_mut()
        .ok_or("missing categories")?
    {
        for (name, value) in entries.as_object_mut().ok_or("missing category entries")? {
            let next = &after["categories"][&*table][&*name];
            if value != next {
                require(
                    *value == "capacity" && *next == "transfer_limit",
                    "unexpected changed category",
                )?;
                *value = next.clone();
                count += 1;
            }
        }
    }
    require(
        count == 1 && before == after,
        "mutation must change exactly one category",
    )
}

pub fn qualify(
    cli: &Path,
    original: &Path,
    output: &Path,
    old_target: &Path,
    new_target: &Path,
    revision: &str,
) -> Result {
    let cli = cli.canonicalize()?;
    let original = original.canonicalize()?;
    checkout(&original, ORIGINAL, false)?;
    require(!output.try_exists()?, "choose a fresh output directory")?;
    let pinned = run(Command::new("git")
        .current_dir(&original)
        .args(["show", &format!("{ORIGINAL}:{SOURCE}")]))?;
    require(
        pinned == fs::read(original.join(SOURCE))?,
        "source differs from pinned git blob",
    )?;
    let tmp = tempfile::tempdir()?;
    let work = tmp.path().canonicalize()?;
    let lane = whole().join("native-error");
    let mutant = work.join("original-mutant");
    mutate_source(&original, &mutant)?;
    let selection = lane.join("selection.json");
    let mut updated = load(&selection)?;
    updated["sourceSha256"] = json!(sha(&mutant.join(SOURCE))?);
    let changed_selection = work.join("mutant-selection.json");
    write(&changed_selection, &updated)?;
    for (role, src, pin) in [
        ("baseline", &original, &selection),
        ("mutant", &mutant, &changed_selection),
    ] {
        run(Command::new(&cli)
            .args(["recover", "finite-error-domain"])
            .arg(src.join(SOURCE))
            .arg(pin)
            .arg(work.join(role)))?;
    }
    check_delta(&work.join("baseline"), &work.join("mutant"))?;
    let rust = stdout(Command::new("rustc").args(["+1.97.1", "-vV"]))?;
    let mut receipt = json!({
        "format":"marmish-native-error-evidence-v1",
        "originalRevision":ORIGINAL,
        "frameworkRevision":revision,
        "rustc":rust.lines().collect::<Vec<_>>(),
        "callerSha256":sha(&lane.join("probe.rs"))?,
        "scope":"finite error API and display/source/From behavior; default debug build",
        "variants":10,
        "typedCategories":{"copy":14,
        "wire":10},
        "profiles":{}
    });
    let mut observations = vec![];
    for (role, src, category) in [
        ("baseline", &original, "capacity"),
        ("mutant", &mutant, "transfer_limit"),
    ] {
        let inputs = work.join(format!("inputs-{role}"));
        let manifest = compose(&inputs, &work.join(role))?;
        if role == "baseline" {
            for name in [
                SPEC,
                "native-error-package.json",
                "native-error-v1/Cargo.lock",
            ] {
                require(
                    fs::read(inputs.join(name))? == fs::read(portable().join(name))?,
                    format!("composition differs for {name}"),
                )?;
            }
        }
        let generated = work.join(format!("generated-{role}"));
        run(Command::new(&cli)
            .args(["build", "native-workspace", "rust"])
            .arg(&manifest)
            .arg(&generated))?;
        let upstream = native_probe(src, old_target, &work.join("original-probe"))?;
        let actual = native_probe(&generated, new_target, &work.join("generated-probe"))?;
        let rows = std::str::from_utf8(&actual)?;
        let quota = format!("Copy|Quota|\"copy: {category}\"");
        require(
            upstream == actual
                && rows.lines().count() == 32
                && rows.lines().any(|l| l.starts_with(&quota)),
            format!("incorrect {role} error observations"),
        )?;
        let head = stdout(
            Command::new("git")
                .current_dir(src)
                .args(["rev-parse", "HEAD"]),
        )?;
        receipt["profiles"][role] = json!({
            "sourceRevision":head.trim(),
            "sourceSha256":sha(&src.join(SOURCE))?,
            "graphSha256":sha(&inputs.join(GRAPH))?,
            "overlaySha256":sha(&inputs.join(OVERLAY))?,
            "workspaceSpecSha256":sha(&manifest)?,
            "workspaceLockSha256":sha(&generated.join("workspace.lock"))?,
            "originalAndGeneratedOutputSha256":bytes_sha(&actual),
            "matchedRows":32,
            "quotaDisplay":category
        });
        observations.push((role, actual));
    }
    require(
        observations[0].1 != observations[1].1,
        "mutant preserved native error output",
    )?;
    checkout(&original, ORIGINAL, false)?;
    let parent = std::env::current_dir()?
        .join(output)
        .parent()
        .ok_or("missing output parent")?
        .to_owned();
    fs::create_dir_all(&parent)?;
    let staged = tempfile::tempdir_in(parent)?;
    for (role, raw) in observations {
        fs::write(staged.path().join(format!("{role}.txt")), raw)?;
    }
    write(&staged.path().join("receipt.json"), &receipt)?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    Ok(())
}
