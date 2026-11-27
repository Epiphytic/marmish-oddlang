//! Interpret the Linux kernel observations used by the local bridge qualifier.
use clap::{Parser, Subcommand};
use public_data_qualification::{Result, require};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, BufRead, Read},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Read the traced process and locking calls from strace -f -qq on stdin.
    Trace { pid: u32, bridge: PathBuf },
    /// Check procfs status and limits plus NUL-delimited descriptor targets on stdin.
    Process { pid: u32, uid: u32, state: PathBuf },
}

fn syscalls(input: impl BufRead, pid: u32, bridge: &Path) -> Result<BTreeSet<String>> {
    let pid = pid.to_string();
    // strace -xx encodes every path byte, including quotes and non-UTF-8 bytes.
    let mut executable = String::from("execve(\"");
    for byte in bridge.as_os_str().as_bytes() {
        use std::fmt::Write;
        write!(executable, "\\x{byte:02x}")?;
    }
    executable.push_str("\",");
    let mut calls = BTreeSet::new();
    let (mut exec, mut exit, mut locked) = (false, false, false);
    let mut partial = None;
    for line in input.lines() {
        let line = line?;
        let Some((process, event)) = line.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        if process != pid {
            continue;
        }
        let event = event.trim();
        let completed;
        let event = if let Some(start) = event.strip_suffix("<unfinished ...>") {
            partial = Some(start.trim_end().to_owned());
            continue;
        } else if let Some(resume) = event.strip_prefix("<... ") {
            let (name, rest) = resume
                .split_once(" resumed>")
                .ok_or("malformed resumed syscall")?;
            let start = partial.take().ok_or("missing unfinished syscall")?;
            require(
                start.split_once('(').map(|(n, _)| n) == Some(name),
                "resumed syscall differs",
            )?;
            completed = start + rest;
            completed.as_str()
        } else {
            event
        };
        let Some((name, _)) = event.split_once('(') else {
            continue;
        };
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        let success = event
            .rsplit_once('=')
            .is_some_and(|(_, ret)| ret.trim() == "0");
        if matches!(name, "execve" | "execveat") && success {
            exec = event.starts_with(&executable);
            locked = false;
            calls.clear();
        }
        if !exec {
            continue;
        }
        calls.insert(name.to_owned());
        if event.starts_with("exit_group(0)") {
            exit = true;
            break;
        }
        locked |= matches!(name, "fcntl" | "fcntl64")
            && success
            && event
                .split(',')
                .nth(1)
                .is_some_and(|op| matches!(op.trim(), "F_OFD_SETLK" | "F_OFD_SETLKW"))
            && (event.contains("l_type=F_WRLCK") || event.contains("l_type=F_RDLCK"));
    }
    require(exec, "missing successful bridge exec")?;
    require(exit, "missing successful bridge exit")?;
    require(locked, "missing successful Linux OFD lock syscall")?;
    Ok(calls)
}

fn process(status: &str, limits: &str, targets: &[u8], uid: u32, state: &Path) -> Result<Value> {
    let field = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
    };
    let effective = field("Uid:")
        .and_then(|v| v.split_whitespace().nth(1))
        .ok_or("missing effective UID")?
        .parse::<u32>()?;
    let caps = field("CapEff:").ok_or("missing effective capabilities")?;
    require(
        uid != 0 && effective == uid && u64::from_str_radix(caps, 16)? == 0,
        "reference process did not drop privileges",
    )?;
    let limits = limits
        .lines()
        .find_map(|line| line.strip_prefix("Max open files"))
        .ok_or("missing descriptor limits")?;
    require(
        limits.split_whitespace().take(2).eq(["256", "256"]),
        "unexpected bridge resource limits",
    )?;
    let mut prefix = state.as_os_str().as_bytes().to_vec();
    prefix.push(b'/');
    let mut names = BTreeSet::new();
    for target in targets.split(|&byte| byte == 0) {
        if let Some(relative) = target.strip_prefix(prefix.as_slice()) {
            names.insert(format!("$STATE/{}", str::from_utf8(relative)?));
        }
    }
    for required in [
        "frontend.lock",
        "engine.sqlite3",
        "engine.sqlite3-wal",
        "engine.sqlite3-shm",
    ] {
        require(
            names.contains(&format!("$STATE/{required}")),
            format!("missing live descriptor {required}"),
        )?;
    }
    Ok(
        json!({"effectiveUid":effective,"effectiveCapabilities":caps,"descriptorLimits":[256,256],"stateDescriptorsAfterReply":names}),
    )
}

fn main() -> Result<()> {
    let observation = match Cli::parse().command {
        Action::Trace { pid, bridge } => json!(syscalls(io::stdin().lock(), pid, &bridge)?),
        Action::Process { pid, uid, state } => {
            let root = PathBuf::from(format!("/proc/{pid}"));
            let mut targets = Vec::new();
            io::stdin().lock().read_to_end(&mut targets)?;
            process(
                &fs::read_to_string(root.join("status"))?,
                &fs::read_to_string(root.join("limits"))?,
                &targets,
                uid,
                &state,
            )?
        }
    };
    println!("{observation}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_trace_evidence_for_the_selected_pid() {
        let exe = r#"42 execve("\x2f\x62", [], []) = 0"#;
        let lock = "42 fcntl(3, F_OFD_SETLK, {l_type=F_WRLCK, l_start=0}) = 0";
        let end = "42 exit_group(0) = ?";
        let observe = |s: &str| syscalls(s.as_bytes(), 42, Path::new("/b"));
        let trace = format!("{exe}\n{lock}\n{end}\n");
        assert_eq!(
            observe(&trace).unwrap(),
            BTreeSet::from(["execve".into(), "fcntl".into(), "exit_group".into()])
        );
        let interleaved = trace.replace(lock, "42 fcntl(3, F_OFD_SETLK, {l_type=F_WRLCK} <unfinished ...>\n57 exit_group(0) = ?\n42 <... fcntl resumed>) = 0");
        assert!(observe(&interleaved).is_ok());
        for changed in [
            format!("{lock}\n{exe}\n{end}"),
            format!("{exe}\n{end}\n{lock}"),
            format!("{exe}\n{lock}\n42 execve(\"other\", [], []) = 0\n{end}"),
            trace.replace("42 fcntl", "57 fcntl"),
            trace.replace("F_WRLCK", "F_UNLCK"),
            trace.replace("F_OFD_SETLK", "F_SETLK"),
            trace.replace("}) = 0", "}) = -1 EAGAIN"),
            trace.replace("\\x62", "\\x62\\x32"),
            trace.replace("[]) = 0", "[]) = -1 ENOENT"),
            trace.replace("exit_group(0)", "exit_group(1)"),
        ] {
            assert!(observe(&changed).is_err(), "{changed}");
        }
    }

    #[test]
    fn checks_process_evidence_and_normalizes_only_the_state_directory() {
        let status = "Uid:\t1000 501 501 501\nCapEff:\t0000000000000000\n";
        let limits = "Max open files            256                  256                  files";
        let names = b"/state/engine.sqlite3\0/state/frontend.lock\0/state/engine.sqlite3-wal\0/state/engine.sqlite3-shm\0/state/jobs\0/state-elsewhere/other\0";
        let check = |s: &str, l: &str, n: &[u8]| process(s, l, n, 501, Path::new("/state"));
        let result = check(status, limits, names).unwrap();
        let archive: Value = serde_json::from_str(include_str!(
            "../../../linux/aarch64-unknown-linux-gnu/effects.json"
        ))
        .unwrap();
        for (field, value) in result.as_object().unwrap() {
            assert_eq!(value, &archive[field]);
        }
        assert!(check(&status.replace("501", "502"), limits, names).is_err());
        assert!(
            check(
                &status.replace("0000000000000000", "0000000000000001"),
                limits,
                names
            )
            .is_err()
        );
        assert!(check(status, &limits.replacen("256", "512", 1), names).is_err());
        assert!(
            check(
                status,
                limits,
                names.strip_prefix(b"/state/engine.sqlite3\0").unwrap()
            )
            .is_err()
        );
    }
}
