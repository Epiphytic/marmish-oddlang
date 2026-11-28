//! Run only the classified help/version observations in the existing terminal sandbox.
use super::{ROLES, arguments, classify};
use public_data_qualification::{
    Result,
    assembly::{bytes_sha, seatbelt_policy},
    capture, list, require, text,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, clap::ValueEnum)]
pub(super) enum Sandbox {
    Seatbelt,
    Bwrap,
    SudoBwrap,
}

impl Sandbox {
    fn command(&self, binary: &Path, home: &Path) -> Result<Command> {
        let binary = binary.canonicalize()?;
        if matches!(self, Self::Seatbelt) {
            let mut cmd = Command::new("/usr/bin/sandbox-exec");
            cmd.arg("-p")
                .arg(seatbelt_policy(home, &[&binary], &[])?)
                .arg(binary)
                .current_dir(home)
                .env_clear()
                .env("HOME", home)
                .env("LANG", "C")
                .env("PATH", "/usr/bin:/bin");
            // umask is async-signal-safe and only changes the child.
            unsafe {
                cmd.pre_exec(|| {
                    libc::umask(0o077);
                    Ok(())
                });
            }
            return Ok(cmd);
        }
        let mut cmd = if matches!(self, Self::SudoBwrap) {
            let mut cmd = Command::new("sudo");
            cmd.args(["-n", "bwrap"]);
            cmd
        } else {
            Command::new("bwrap")
        };
        cmd.args([
            "--unshare-net",
            "--unshare-ipc",
            "--unshare-pid",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--dev",
            "/dev",
            "--ro-bind",
        ])
        .arg(home)
        .args([
            "/home",
            "--chdir",
            "/home",
            "--setenv",
            "HOME",
            "/home",
            "--setenv",
            "LANG",
            "C",
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
        ]);
        if Path::new("/lib64").exists() {
            cmd.args(["--ro-bind", "/lib64", "/lib64"]);
        }
        let subject =
            Path::new("/subject").join(binary.file_name().ok_or("missing executable name")?);
        cmd.arg("--ro-bind")
            .arg(binary)
            .arg(&subject)
            .args(["--remount-ro", "/"]);
        if matches!(self, Self::SudoBwrap) {
            cmd.args(["setpriv", "--reuid"])
                .arg(unsafe { libc::getuid() }.to_string())
                .arg("--regid")
                .arg(unsafe { libc::getgid() }.to_string())
                .args(["--clear-groups", "--no-new-privs", "--"]);
        }
        cmd.arg(subject);
        Ok(cmd)
    }
}

#[derive(clap::Args)]
pub struct Args {
    #[arg(value_enum)]
    pub(super) sandbox: Sandbox,
    pub(super) originals: PathBuf,
    pub(super) binaries: PathBuf,
    pub(super) scratch: PathBuf,
}

impl Args {
    pub fn run(&self, archive: &Value) -> Result<Value> {
        let selected = classify(archive)?;
        let scratch = self.scratch.canonicalize()?;
        let mut results = json!({});
        for role in ROLES {
            let home = scratch.join(role);
            fs::create_dir(&home)?;
            let mut observations = Vec::new();
            for row in list(&selected[role][0])? {
                let argv = arguments(row)?;
                require(
                    row.as_object().is_some_and(|obj| obj.len() == 4),
                    "unexpected terminal observation fields",
                )?;
                let stdout = text(&row["stdout"])?.as_bytes();
                let stderr = text(&row["stderr"])?.as_bytes();
                require(
                    stdout.is_ascii() && stderr.is_ascii(),
                    "non-ASCII archived terminal output",
                )?;
                for dir in [&self.originals, &self.binaries] {
                    let output = capture(
                        self.sandbox.command(&dir.join(role), &home)?.args(&argv),
                        8,
                        None,
                    )?;
                    require(
                        output.status.code() == Some(0)
                            && output.stdout == stdout
                            && output.stderr == stderr,
                        format!(
                            "terminal observation differs for {} {role} {argv:?}",
                            dir.display()
                        ),
                    )?;
                    require(
                        fs::read_dir(&home)?.next().is_none(),
                        "terminal case changed its home",
                    )?;
                }
                observations.push(json!({"args":argv,"exit":0,"stdoutSha256":bytes_sha(stdout),"stderrSha256":bytes_sha(stderr)}));
            }
            results[role] = json!({"observations":observations,"excludedArchivedRows":selected[role][1],"excludedPostParseControl":selected[role][2]});
        }
        Ok(results)
    }
}
