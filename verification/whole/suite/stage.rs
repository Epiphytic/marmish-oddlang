//! Stage the pinned native test sources used by the named-suite qualification.
use public_data_qualification::{
    Result,
    assembly::{ORIGINAL, bytes_sha},
    manifest_product, require, stdout,
};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path, process::Command};

fn source(repo: &Path, path: &str) -> Result<String> {
    stdout(
        Command::new("git")
            .current_dir(repo)
            .args(["show", &format!("{ORIGINAL}:{path}")]),
    )
}
fn offset(source: &str, marker: &str) -> Result<usize> {
    let mut matches = source.match_indices(marker);
    let start = matches.next().ok_or("missing upstream section")?.0;
    require(matches.next().is_none(), "ambiguous upstream section")?;
    Ok(start)
}
pub fn vault_engine(original: &Path, product: &Path) -> Result<Value> {
    let vault = source(original, "crates/marmish-vault/src/lib.rs")?;
    let module = &vault[offset(&vault, "#[cfg(test)]\nmod tests {")?..];
    let engine = source(
        original,
        "crates/marmish-engine/src/process_limits_tests.rs",
    )?;
    let start = offset(&engine, "const CASE:")?;
    let mac = offset(&engine, "#[cfg(target_os = \"macos\")]\nfn high_soft()")?;
    let low = offset(
        &engine,
        "#[test]\nfn lower_soft_hard_and_descriptor_flags_are_unchanged()",
    )?;
    let stop = offset(
        &engine,
        "#[test]\nfn normalized_startup_executes_offline_exact_bytes_and_status()",
    )?;
    require(
        start < mac && mac < low && low < stop,
        "unexpected upstream process limit module order",
    )?;
    let helpers = &engine[start..mac];
    let cases = &engine[low..stop];
    let mut digests = serde_json::Map::new();
    for (name, part, pin) in [
        (
            "vaultValueModule",
            module,
            "5389218be38e60288371fe63a79ae71d2e9dbb8408a34bc53e8010f03117359c",
        ),
        (
            "engineLimitHelpers",
            helpers,
            "f27e24a823249163fa4e7d99634429edf50488b8706094bcbfe19bdbb599b11f",
        ),
        (
            "engineLimitCases",
            cases,
            "c8344cac4a33a60ec110a385c4a61087ccfee2f8a52e41395cab4513c852d570",
        ),
    ] {
        require(
            bytes_sha(part.as_bytes()) == pin,
            format!("upstream {name} bytes changed"),
        )?;
        digests.insert(name.to_owned(), json!(pin));
    }
    let files = [
        (
            "marmish-vault/tests",
            format!("pub use marmish_vault::*;\n{module}"),
        ),
        (
            "marmish-engine/tests",
            format!(
                "mod process_limits_tests {{\nuse marmish_engine::initialize_process_limits;\nuse std::{{os::fd::AsRawFd, process::Command}};\n{helpers}{cases}}}\n"
            ),
        ),
    ];
    for (dir, _) in &files {
        let dest = product.join(dir);
        require(
            dest.parent().is_some_and(Path::is_dir) && !dest.try_exists()?,
            "expected unstaged target package",
        )?;
    }
    let mut paths = Vec::new();
    for (dir, body) in files {
        fs::create_dir(product.join(dir))?;
        let path = Path::new(dir).join("selected_upstream.rs");
        fs::write(product.join(&path), body)?;
        paths.push(path);
    }
    Ok(json!({"paths":paths,"digests":digests}))
}

pub fn parser(original: &Path, product: &Path) -> Result<Value> {
    let (leaf, prefix) = manifest_product::leaf(&product.join("marmish-native"))?;
    let tests = manifest_product::tests(original)?;
    let vector = manifest_product::VECTOR;
    let bytes = fs::read(original.join(vector))?;
    let test_digest = bytes_sha(&tests);
    let vector_digest = bytes_sha(&bytes);
    require(
        test_digest == "6012150c30678a0ccaddd4475249132528d174ab0bf62750dd9c9e5073dcfae8"
            && vector_digest == "b07696eda226714f8bd5f6a725be662d1a7df4ec1693c2bfd30a78df0dc3b6c1",
        "original parser test bytes or vectors changed",
    )?;
    let target = product.join(vector);
    fs::create_dir_all(target.parent().ok_or("missing vector parent")?)?;
    let mut code = fs::OpenOptions::new().append(true).open(&leaf)?;
    fs::File::create_new(target)?.write_all(&bytes)?;
    code.write_all(b"\n")?;
    code.write_all(&tests)?;
    Ok(json!({
        "leaf":leaf.strip_prefix(product)?, "prefix":prefix, "vector":vector,
        "digests":{"manifestTests":test_digest,"releaseVersionVectors":vector_digest}
    }))
}

fn append_module(crate_root: &Path, leaf: &str, module: &str) -> Result<()> {
    let path = crate_root.join(leaf);
    require(
        !fs::read_to_string(&path)?.contains(module),
        "selected test is already staged",
    )?;
    fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .write_all(module.as_bytes())?;
    Ok(())
}

pub fn reader(original: &Path, prototype: &Path) -> Result<Value> {
    use public_data_qualification::buffered;
    let leaf = "src/group_0/__projection_0.rs";
    let module = buffered::original_test(&buffered::reader_source(original)?)?;
    append_module(prototype, leaf, &module)?;
    Ok(json!({"leaf":leaf,"bodySha256":buffered::READER_TEST_SHA}))
}

pub fn mdk(original: &Path, product: Option<&Path>) -> Result<Value> {
    let text = source(original, "crates/marmish-mdk/src/tests.rs")?;
    let head = "#[test]\nfn synthetic_json_and_ndjson_are_bounded_and_structural_only()";
    let start = offset(&text, head)?;
    let end = text[start + head.len()..]
        .find("\n#[test]")
        .ok_or("missing end of MDK case")?
        + start
        + head.len();
    let body = &text[start..end];
    let digest = bytes_sha(body.as_bytes());
    require(
        digest == "04916e33e73f6e8856688823f8af5af965677a7403e195477de9f4a19d1a6ad7",
        "unchanged MDK test digest differs",
    )?;
    let leaf = "src/group_2/__projection_2.rs";
    if let Some(product) = product {
        let module = format!(
            "\n#[cfg(test)] mod tests {{\nuse super::{{parse_json, parse_ndjson, OUTPUT_LIMIT}};\nuse crate::AdapterError;\n{body}\n}}\n"
        );
        append_module(product, leaf, &module)?;
    }
    Ok(json!({"leaf":leaf,"bodySha256":digest}))
}
