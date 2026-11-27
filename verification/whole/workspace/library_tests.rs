//! Attach the original core contracts and stream tests after the source-free build.
use public_data_qualification::{
    Result, assembly::bytes_sha, list, load, portable, require, whole,
};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};

const WIRE_CALLER: &str = r#"
#[cfg(test)]
mod wire_caller {
    use crate::wire::*;
    use crate::codec::{Codec, EncodedBlock};
    use crate::{MAX_BLOCK, MAX_CONTENT, MAX_OFFSET, WINDOW_BYTES};
    const MAX_ARGV_BYTES: usize = 16384;
    const MAX_ARGV_ITEMS: usize = 128;
    const MAX_AGE_SECONDS: u64 = 604800;
    const FUTURE_SKEW_SECONDS: u64 = 60;
    include!("original_wire_test_module.rs");
}
"#;
const COPY_CALLER: &str = r#"
#[cfg(test)]
mod copy_caller {
    use crate::{MAX_BLOCK, MAX_OFFSET, codec::Codec, copy_protocol::*};
    use sha2::{Digest, Sha256};
    const CONTEXT_DOMAIN: &[u8] = b"marmish-copy-context/1\0";
    include!("original_copy_test_module.rs");
}
"#;

fn test_module(original: &Path, name: &str) -> Result<String> {
    let source = fs::read_to_string(original.join(format!("crates/marmish-stream/src/{name}.rs")))?;
    let marker = "#[cfg(test)]\nmod tests {";
    let (prefix, rest) = source
        .split_once(marker)
        .ok_or("missing upstream test module")?;
    require(
        !rest.contains(marker),
        format!("ambiguous upstream test module: {name}"),
    )?;
    let suite = &source[prefix.len()..];
    if name == "copy_protocol" {
        Ok(suite
            .split_once("\nmod path_encoding {")
            .ok_or("missing copy test boundary")?
            .0
            .to_owned())
    } else {
        Ok(suite.to_owned())
    }
}

fn append(path: &Path, bytes: &[u8]) -> Result {
    fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

pub fn stage(
    product: &Path,
    original: &Path,
    manifest: &Path,
    wire: bool,
    copy: bool,
) -> Result<Value> {
    let component_manifest = load(&portable().join(manifest))?;
    let components = list(&component_manifest["components"])?;
    let core = product.join("marmish-core/tests");
    let stream = product.join("marmish-stream");
    let src = stream.join("src");
    let contracts = fs::read(original.join("crates/marmish-core/tests/contracts.rs"))?;
    let mut hashes = json!({"coreContracts":bytes_sha(&contracts)});
    fs::create_dir(&core)?;
    fs::write(core.join("contracts.rs"), contracts)?;
    for (name, imports) in [
        ("codec", "Error, MAX_BLOCK, MAX_OFFSET"),
        ("windows", "Error, MAX_BLOCK, MAX_RECORDS, WINDOW_BYTES"),
    ] {
        let indices: Vec<_> = components
            .iter()
            .enumerate()
            .filter(|(_, c)| c["module"] == name)
            .map(|(i, _)| i)
            .collect();
        let [index] = indices.as_slice() else {
            return Err(format!("expected one {name} component").into());
        };
        let suite = test_module(original, name)?;
        hashes[name] = bytes_sha(suite.as_bytes()).into();
        let source = src.join(format!("group_{index}/__projection_{index}.rs"));
        append(
            &source,
            format!("\n#[cfg(test)]\nuse crate::{{{imports}}};\n{suite}").as_bytes(),
        )?;
    }
    if wire {
        let suite = test_module(original, "wire")?;
        hashes["wire"] = bytes_sha(suite.as_bytes()).into();
        fs::write(src.join("original_wire_test_module.rs"), suite)?;
        append(&src.join("lib.rs"), WIRE_CALLER.as_bytes())?;
    }
    if copy {
        let suite = test_module(original, "copy_protocol")?;
        let exact = fs::read(whole().join("copy-frame/exact.rs"))?;
        hashes["copyProtocol"] = bytes_sha(suite.as_bytes()).into();
        hashes["exactCopyFrames"] = bytes_sha(&exact).into();
        fs::write(src.join("original_copy_test_module.rs"), suite)?;
        fs::create_dir(stream.join("tests"))?;
        fs::write(stream.join("tests/exact_copy_frames.rs"), exact)?;
        append(&src.join("lib.rs"), COPY_CALLER.as_bytes())?;
    }
    Ok(hashes)
}
