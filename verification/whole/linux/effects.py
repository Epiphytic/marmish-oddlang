#!/usr/bin/env python3
"""Observe a reference bridge Init under a read-only root on a disposable Linux VM."""
import argparse
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile



def child_of(pid, name):
    children = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
    for child in children:
        child = int(child)
        if Path(f"/proc/{child}/comm").read_text().strip() == name[:15]:
            return child
        found = child_of(child, name)
        if found:
            return found
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("driver", type=Path)
    parser.add_argument("bin_dir", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("oracle", args.driver.resolve())
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    fixture = json.loads(oracle.FIXTURE.read_bytes())
    uid, gid = os.getuid(), os.getgid()
    if uid == 0:
        parser.error("an ordinary user on a disposable guest with sudo for mount setup is required")
    bins = args.bin_dir.resolve()
    with tempfile.TemporaryDirectory(prefix="bridge-linux-effects-") as run:
        parent = Path(run)
        run = parent / "disposable"
        run.mkdir(mode=0o700)
        state, cwd, log = run / "state", run / "cwd", parent / "trace"
        cwd.mkdir(mode=0o700)
        state.mkdir(mode=0o700)
        sandbox = ["/usr/bin/bwrap", "--ro-bind", "/", "/", "--bind", state, state,
                   "--", "/usr/bin/setpriv", "--reuid", str(uid), "--regid", str(gid),
                   "--clear-groups", "--no-new-privs", "--", "/usr/bin/prlimit", "--nofile=256:256", "--"]
        denied = subprocess.run(["sudo", "-n", *sandbox, "/usr/bin/touch", cwd / "outside"],
                                env=oracle.ENVIRONMENT, capture_output=True, timeout=10)
        if not denied.returncode or b"Read-only file system" not in denied.stderr or (cwd / "outside").exists():
            raise RuntimeError("outside write probe did not fail with EROFS")
        before, _ = oracle.inspect_state(run)
        oracle.compare("lock before Init", "absent", before["frontend_lock"])
        flags = ["-f", "-qq", "-s", "0", "-e", "trace=%process,flock,fcntl", "-o", log]
        bridge = bins / "marmish-engine-bridge"
        cmd = ["sudo", "-n", "/usr/bin/strace", *flags, *sandbox, bridge]
        request = fixture["request"].replace("@VERSION@", "1")
        for key, path in {"STATE": state, "CWD": cwd, "WORKER": bins / "marmish-engine-worker"}.items():
            request = request.replace("@" + key + "@", json.dumps(str(path)))
        with subprocess.Popen(cmd, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, env=oracle.ENVIRONMENT, umask=oracle.UMASK,
                              preexec_fn=oracle.constrain_child) as child:
            try:
                child.stdin.write(request.encode())
                child.stdin.flush()
                oracle.compare("restricted Init", fixture["replies"]["initialized"],
                               oracle.read_reply(child.stdout))
                pid = child_of(child.pid, bridge.name)
                if not pid:
                    raise RuntimeError("tracer tree does not contain the bridge")
                proc = Path(f"/proc/{pid}")
                status = dict(line.split(":\t", 1) for line in (proc / "status").read_text().splitlines() if ":\t" in line)
                if int(status["Uid"].split()[1]) != uid or int(status["CapEff"], 16):
                    raise RuntimeError("reference process did not drop privileges")
                limits = next(l for l in (proc / "limits").read_text().splitlines() if l.startswith("Max open files")).split()
                if limits[3:5] != ["256", "256"]:
                    raise RuntimeError(f"unexpected bridge resource limits {limits}")
                # Yama prevents the observer from inspecting a process launched through sudo.
                output = subprocess.check_output(["sudo", "-n", "/usr/bin/find", proc / "fd",
                                                  "-maxdepth", "1", "-type", "l", "-printf", "%l\\0"])
                names = sorted({os.fsdecode(n).replace(str(state), "$STATE") for n in output.split(b"\0")
                                if n.startswith(os.fsencode(state) + b"/")})
                needed = {"$STATE/frontend.lock", "$STATE/engine.sqlite3", "$STATE/engine.sqlite3-wal",
                          "$STATE/engine.sqlite3-shm"}
                if not needed.issubset(names):
                    raise RuntimeError(f"incomplete live state descriptors {names}")
                live, _ = oracle.inspect_state(run)
                oracle.compare_state("live", fixture["states"]["live"], live)
                # A BSD probe can succeed while the actual Linux OFD lock is held.
                # This reproduces the defect in the original cross-platform verifier.
                with (state / "frontend.lock").open("rb") as lock_file:
                    fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    fcntl.flock(lock_file, fcntl.LOCK_UN)
                stdout, stderr = child.communicate(timeout=oracle.TIMEOUT)
                oracle.compare("exit", (0, b"", b""), (child.returncode, stdout, stderr))
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=oracle.TIMEOUT)
        closed, _ = oracle.inspect_state(run)
        oracle.compare_state("closed", fixture["states"]["closed"], closed)
        # Root's tracer writes only this temporary log. Keep syscall names and normalized exec paths.
        trace = subprocess.check_output(["sudo", "-n", "/bin/cat", log], text=True)
        entries = [line.split(maxsplit=1) for line in trace.splitlines() if line.strip()]
        events = [call for process, call in entries if process == str(pid)]
        if not any(call.startswith("execve(") and str(bridge) in call for call in events):
            raise RuntimeError("kernel did not trace the original binary exec")
        if not any(call.startswith("exit_group(0)") for call in events):
            raise RuntimeError("kernel did not trace successful bridge exit")
        calls = sorted({m.group(1) for call in events if (m := re.match(r"(\w+)\(", call))})
        measured = {"process": bridge.name, "effectiveUid": uid, "effectiveCapabilities": status["CapEff"],
                    "descriptorLimits": [256, 256], "stateDescriptorsAfterReply": names,
                    "lockBeforeInit": before["frontend_lock"], "bsdProbeWhileOfdHeld": "available",
                    "lockDuringInit": live["frontend_lock"], "lockAfterExit": closed["frontend_lock"],
                    "bridgeProcessAndLockSyscalls": calls, "outsideCreateControl": "EROFS",
                    "filesystemPolicy": "read-only host root with one writable state directory",
                    "initialState": "existing empty mode 0700 directory"}
        args.output.write_text(json.dumps(measured, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
