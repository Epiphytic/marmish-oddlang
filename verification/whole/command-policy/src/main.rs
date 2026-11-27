//! Offline command policy qualification against the upstream and generated observers.
mod observers;
mod recover;
mod replay;

use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PIN: &str = "af54339d250290490d62d4475a77539b7e739071";
const TOOLCHAIN: &str = "1.97.1";

fn output(command: &mut Command) -> Result<String> {
    let response = command.output()?;
    if !response.status.success() {
        return Err(format!(
            "{command:?}: {} {}",
            response.status,
            String::from_utf8_lossy(&response.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(response.stdout)?)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("{command:?}: {status}").into());
    }
    Ok(())
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match args.split_first().and_then(|(mode, rest)| mode.to_str().map(|s|(s,rest))) {
        Some(("replay",[native, generated, room])) => {
            println!("{}",replay::policy(&native.canonicalize()?, &generated.canonicalize()?)?);
            println!("{} room file cases", replay::room(&room.canonicalize()?)?);
        }
        Some(("profile",[compiler, dest])) => recover::profile_workspace(&compiler.canonicalize()?, &std::path::absolute(dest)?)?,
        Some(("recover",[compiler, original, dest])) => recover::workspace(&compiler.canonicalize()?, &original.canonicalize()?, &std::path::absolute(dest)?)?,
        Some(("qualify",[original, cache, workspace, receipt])) => {
            fs::create_dir_all(cache)?;
            observers::qualify(&original.canonicalize()?, &cache.canonicalize()?, &workspace.canonicalize()?, receipt)?;
        }
        _ => return Err("usage: command-policy-validation profile COMPILER NEW_WORKSPACE | recover COMPILER ORIGINAL NEW_WORKSPACE | qualify ORIGINAL CACHE WORKSPACE RECEIPT | replay NATIVE GENERATED ROOM".into()),
    }
    Ok(())
}
