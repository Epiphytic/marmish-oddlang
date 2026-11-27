//! Validate the producer's selection against its independently pinned canonical suite.
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env, fs, io, sync::LazyLock};
use suite_receipts::{Result, suite};

const STREAM: &str = "marmish-stream/lib/marmish_stream";
static PROJECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(windows|codec)::__projection_\d+\z").unwrap());

fn stream_name(name: &str) -> Result<String> {
    let invalid = || format!("unexpected upstream module mapping: {name}");
    let (prefix, tail) = name.split_once("::tests::").ok_or_else(invalid)?;
    if tail.contains("::tests::") {
        return Err(invalid().into());
    }
    let module = match prefix {
        "copy_caller" => "copy_protocol",
        "wire_caller" => "wire",
        other => PROJECTION
            .captures(other)
            .and_then(|c| c.get(1))
            .ok_or_else(invalid)?
            .as_str(),
    };
    Ok(format!("{module}::tests::{tail}"))
}

fn registered(harness: &str, listing: &str, mut expected: Vec<String>) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for name in listing
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
    {
        names.push(if harness == STREAM {
            stream_name(name)?
        } else {
            name.to_owned()
        });
    }
    names.sort();
    expected.sort();
    if names != expected {
        return Err(format!("unsafe or incomplete test registration for {harness}").into());
    }
    Ok(names)
}

fn baseline(host: &str) -> Result<(&'static str, Value)> {
    let pin = match host {
        "aarch64-apple-darwin" => {
            "7cfd7edb63815f1e73553786abe5dc0c35535c7b7ea98ea4245db979068c2306"
        }
        "aarch64-unknown-linux-gnu" => {
            "4a472c53a1fea65ea7d5b4cf5eb144081aa33e9936e53ff92baa0b3b7359e941"
        }
        _ => return Err("unknown canonical host".into()),
    };
    let data = fs::read(suite().join(format!("canonical-{host}.json")))?;
    if format!("{:x}", Sha256::digest(&data)) != pin {
        return Err("pinned canonical named baseline changed".into());
    }
    Ok((pin, serde_json::from_slice(&data)?))
}

fn input<T: DeserializeOwned>() -> Result<T> {
    Ok(serde_json::from_reader(io::stdin().lock())?)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let value = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["stream-names"] => {
            let names: Vec<String> = input()?;
            serde_json::to_value(
                names
                    .iter()
                    .map(|name| stream_name(name))
                    .collect::<Result<Vec<_>>>()?,
            )?
        }
        ["registered", harness] => {
            let (listing, expected): (String, Vec<String>) = input()?;
            serde_json::to_value(registered(harness, &listing, expected)?)?
        }
        ["baseline", host] => baseline(host)?.1,
        ["check-baseline", host] => {
            let (pin, canonical) = baseline(host)?;
            if input::<Value>()? != canonical["harnesses"] {
                return Err("candidate named baseline differs from the canonical selection".into());
            }
            Value::from(pin)
        }
        _ => {
            return Err(
                "expected stream-names, registered HARNESS, baseline HOST or check-baseline HOST"
                    .into(),
            );
        }
    };
    serde_json::to_writer(io::stdout().lock(), &value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_names_have_exact_module_boundaries() -> Result<()> {
        for (prefix, module) in [
            ("copy_caller", "copy_protocol"),
            ("wire_caller", "wire"),
            ("windows::__projection_17", "windows"),
            ("codec::__projection_0", "codec"),
        ] {
            assert_eq!(
                stream_name(&format!("{prefix}::tests::nested::case"))?,
                format!("{module}::tests::nested::case")
            );
        }
        for invalid in [
            "no_module",
            "extra::tests::case",
            "codec::__projection_x::tests::case",
            "wire_caller::tests::extra::tests::case",
        ] {
            assert!(stream_name(invalid).is_err());
        }
        Ok(())
    }

    #[test]
    fn pre_run_listing_matches_the_pinned_selection_for_each_host() -> Result<()> {
        for host in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
            let (pin, canonical) = baseline(host)?;
            let observed =
                suite_receipts::read(&suite().join(format!("canonical-public-data-{host}.json")))?;
            assert_eq!(pin, observed["canonicalNamedBaselineSha256"]);
            assert_eq!(canonical["harnesses"], observed["harnesses"]);
            for (harness, names) in observed["preRunRegisteredNames"].as_object().unwrap() {
                let expected: Vec<String> = serde_json::from_value(names.clone())?;
                let mut listing = String::from("ignored: benchmark\r\n");
                for name in expected.iter().rev() {
                    let compiled = name
                        .replacen("copy_protocol::tests::", "copy_caller::tests::", 1)
                        .replacen("wire::tests::", "wire_caller::tests::", 1)
                        .replacen("windows::tests::", "windows::__projection_17::tests::", 1)
                        .replacen("codec::tests::", "codec::__projection_2::tests::", 1);
                    listing.push_str(&format!("{compiled}: test\r\n"));
                }
                assert_eq!(registered(harness, &listing, expected.clone())?, expected);
            }
        }
        Ok(())
    }
}
