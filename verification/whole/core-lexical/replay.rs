use core_qualification::*;
use serde_json::json;
use std::{env, fs, io::Write, path::PathBuf};

const USAGE: &str = "usage: core-lexical-replay OBSERVER ORIGINAL CANDIDATE HOST NEW_OUTPUT";

fn main() -> Result {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(Into::into).collect();
    if args
        .iter()
        .any(|arg| arg.as_os_str() == "--help" || arg.as_os_str() == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    let [framework, original, candidate, host, output] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let host = host.to_str().ok_or("invalid host")?;
    require(HOSTS.contains(&host), "expected a qualified ARM64 host")?;
    let original = original.canonicalize()?;
    let candidate = candidate.canonicalize()?;
    fs::create_dir(output)?;
    let dest = output.canonicalize()?;
    pinned_git(&original, ORIGINAL)?;
    let before = snapshot(&candidate)?;
    let observer = Observer::new(&framework.canonicalize()?, LEXICAL_OBSERVER)?;
    for (label, root) in [("original", &original), ("generated", &candidate)] {
        observer.capture(
            root,
            host,
            &dest.join(format!("{label}.json")),
            label == "original",
            false,
        )?;
    }
    observer.check()?;
    let after = snapshot(&candidate)?;
    require(before == after, "candidate changed while observing")?;
    pinned_git(&original, ORIGINAL)?;
    let temp = tempfile::tempdir()?;
    for name in object(&before["generatedFiles"])?.keys() {
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::copy(candidate.join(name), path)?;
    }
    let mut source = fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("src/lib.rs"))?;
    source.write_all(b"\n// provenance control\n")?;
    let rejection = snapshot(temp.path())
        .err()
        .ok_or("changed source accepted under original package lock")?
        .to_string();
    require(
        rejection == "generated file changed: src/lib.rs",
        &rejection,
    )?;
    save(
        &dest.join("receipt.json"),
        &json!({
            "host":host,"originalRevision":ORIGINAL,"observerRevision":observer.revision,"projectionRevision":PROJECTION,
            "before":before,"after":after,"mutation":{"file":"src/lib.rs","outcome":rejection},
            "reportsSha256":{"original":sha(&dest.join("original.json"))?,"generated":sha(&dest.join("generated.json"))?}
        }),
    )
}
