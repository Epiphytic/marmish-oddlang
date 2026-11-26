"""Observe a live initialized original using the macOS sandbox, lsof and ps."""

from contextlib import closing
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time


def live_descriptors(pid, root, bins):
    fields = subprocess.check_output(["/usr/sbin/lsof", "-nP", "-a", "-p", str(pid),
                                      "-Ffatn"], text=True).splitlines()
    rows = []
    row = None
    for field in fields:
        code, value = field[:1], field[1:]
        if code == "f":
            row = {"fd": value}
            rows.append(row)
        elif row is not None and code in {"a", "t", "n"}:
            if code == "n":
                if value.startswith("->0x") or value.startswith("0x"):
                    value = "<kernel handle>"
                value = value.replace(str(root), "$RUN").replace(str(bins), "$BIN")
            row[{"a": "access", "t": "type", "n": "name"}[code]] = value
    return rows


def sample(driver, bins):
    fixture = json.loads(driver.FIXTURE.read_bytes())
    with tempfile.TemporaryDirectory(prefix="whole-bridge-effect-", dir="/tmp") as parent:
        parent = Path(parent).resolve()
        root = parent / "disposable"
        root.mkdir(mode=0o700)
        cwd = root / "cwd"
        cwd.mkdir(mode=0o700)
        state = root / "state"
        # Keep cwd read-only. Init may create the state directory and its descendants.
        policy = '(version 1)(allow default)(deny file-write*)(allow file-write* (subpath ' + json.dumps(str(state)) + '))'
        launch = ["/usr/bin/sandbox-exec", "-p", policy]
        outside = parent / "outside"
        rejected = subprocess.run([*launch, "/usr/bin/touch", outside],
                                  capture_output=True, env=driver.ENVIRONMENT, timeout=driver.TIMEOUT)
        driver.compare("sandbox control", 1, rejected.returncode)
        driver.compare("outside write", False, outside.exists())
        if b"Operation not permitted" not in rejected.stderr:
            raise RuntimeError("outside-write negative control failed for another reason")
        fields = {"CWD": cwd, "STATE": state, "WORKER": bins / "marmish-engine-worker"}
        request = fixture["request"].replace("@VERSION@", "1")
        for name, path in fields.items():
            request = request.replace("@" + name + "@", json.dumps(str(path)))
        with closing(select.kqueue()) as queue, subprocess.Popen([*launch, bins / "marmish-engine-bridge"], cwd=cwd,
                              env=driver.ENVIRONMENT, umask=driver.UMASK, bufsize=0,
                              preexec_fn=driver.constrain_child, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            try:
                deadline = time.monotonic() + driver.TIMEOUT
                while True:
                    executable = subprocess.check_output(
                        ["/bin/ps", "-p", str(child.pid), "-o", "comm="], text=True).strip()
                    if Path(executable).name == "marmish-engine-bridge":
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError("bridge failed to start before tracing deadline")
                    time.sleep(0.01)
                flags = {"fork": select.KQ_NOTE_FORK, "exec": select.KQ_NOTE_EXEC,
                         "exit": select.KQ_NOTE_EXIT}
                watch = select.kevent(child.pid, filter=select.KQ_FILTER_PROC,
                                      flags=select.KQ_EV_ADD | select.KQ_EV_CLEAR,
                                      fflags=flags["fork"] | flags["exec"] | flags["exit"])
                queue.control([watch], 0, 0)
                child.stdin.write(request.encode())
                driver.compare("sandboxed Init reply", fixture["replies"]["initialized"],
                               driver.read_reply(child.stdout))
                observed, _ = driver.inspect_state(root)
                driver.compare_state("sandboxed Init", fixture["states"]["live"], observed)
                fds = live_descriptors(child.pid, root, bins)
                open_names = {row.get("name") for row in fds}
                required = {"$RUN/state/frontend.lock", "$RUN/state/engine.sqlite3",
                            "$RUN/state/engine.sqlite3-wal", "$RUN/state/engine.sqlite3-shm"}
                if not required.issubset(open_names):
                    raise RuntimeError(f"missing live descriptors: {required - open_names}")
                # A checkpoint snapshot may miss processes that exit before Init replies.
                table = subprocess.check_output(["/bin/ps", "-axo", "ppid=,comm="], text=True)
                children = [Path(cols[1]).name for line in table.splitlines()
                            if len(cols := line.split(None, 1)) == 2 and cols[0] == str(child.pid)]
                driver.compare("process parent", os.getpid(), int(subprocess.check_output(
                    ["/bin/ps", "-p", str(child.pid), "-o", "ppid="], text=True)))
                stdout, stderr = child.communicate(timeout=driver.TIMEOUT)
                events = queue.control([], 2, driver.TIMEOUT)
                process_events = sorted(name for name, flag in flags.items()
                                        if any(event.fflags & flag for event in events))
                if "exit" not in process_events:
                    raise RuntimeError("kernel did not report bridge exit")
                driver.compare("sandboxed exit", (0, b"", b""), (child.returncode, stdout, stderr))
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=driver.TIMEOUT)
        final, _ = driver.inspect_state(root)
        driver.compare_state("sandboxed EOF", fixture["states"]["closed"], final)
        return {"at": "after Init reply, before stdin EOF", "direct_children": sorted(children),
                "kernel_events_during_init_through_exit": process_events,
                "file_descriptors": fds, "frontend_lock": observed["frontend_lock"],
                "final_lock": final["frontend_lock"],
                "write_policy": "deny all filesystem writes outside the disposable state root",
                "outside_write_control": "EPERM"}
