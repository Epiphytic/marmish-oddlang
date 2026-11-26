"""Private macOS test workspaces and owned process cleanup."""
import json
import os
from pathlib import Path
import pwd
import signal
import subprocess
import tempfile


SYSTEM_HOME = Path(pwd.getpwuid(os.geteuid()).pw_dir).resolve()
SANDBOX = "/usr/bin/sandbox-exec"


class Instance:
    def __init__(self, root):
        self.root = root
        self.root.mkdir(mode=0o700)
        self.tmp = root / "tmp"
        self.tmp.mkdir(mode=0o700)
        self.profile = root / "sandbox.sb"
        self.profile.write_text(
            "(version 1)\n(allow default)\n"
            f"(deny file-read* (subpath {json.dumps(str(SYSTEM_HOME))}))\n"
            f"(allow file-read-metadata (literal {json.dumps(str(SYSTEM_HOME))}))\n"
            f"(deny file-read* (subpath {json.dumps(str(root.parent))}))\n"
            f"(allow file-read* (subpath {json.dumps(str(root))}))\n"
            f"(allow file-read* (literal {json.dumps(str(root.parent))}))\n"
            "(deny file-write*)\n"
            f"(allow file-write* (subpath {json.dumps(str(root))}) (literal \"/dev/null\"))\n"
            "(deny mach-lookup)\n"
            "(allow mach-lookup (global-name \"com.apple.system.opendirectoryd.libinfo\"))\n"
        )
        self.processes = []

    def start(self, arguments, **kwargs):
        process = subprocess.Popen(
            [SANDBOX, "-f", self.profile, *arguments],
            cwd=self.root,
            env={"HOME": str(self.root), "TMPDIR": str(self.tmp), "LC_ALL": "C", "PATH": "/usr/bin:/bin"},
            start_new_session=True,
            umask=0o077,
            **kwargs,
        )
        self.processes.append(process)
        return process

    def run(self, arguments, input_bytes=b"", timeout=330):
        process = self.start(arguments, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        stdout, stderr = process.communicate(input=input_bytes, timeout=timeout)
        self.check_stopped(process)
        self.processes.remove(process)
        return subprocess.CompletedProcess(arguments, process.returncode, stdout, stderr)

    def prove(self, read_sentinel, write_sentinel):
        inside = self.root / "sandbox-probe"
        result = self.run(["/usr/bin/touch", inside], timeout=5)
        if result.returncode or not inside.is_file():
            raise ValueError("sandbox denied a private file write")
        result = self.run(["/bin/cat", read_sentinel], timeout=5)
        if not result.returncode or result.stdout:
            raise ValueError("sandbox allowed an external read")
        result = self.run(["/usr/bin/touch", write_sentinel], timeout=5)
        if not result.returncode or write_sentinel.exists():
            raise ValueError("sandbox allowed an external write")
        result = self.run(["/bin/ls", "-A", SYSTEM_HOME], timeout=5)
        if not result.returncode or result.stdout:
            raise ValueError("sandbox allowed access to the real home")
        result = self.run(["/usr/bin/security", "default-keychain", "-d", "user"], timeout=5)
        if not result.returncode:
            raise ValueError("sandbox allowed access to the default macOS keychain")

    @staticmethod
    def check_stopped(process):
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return
        raise RuntimeError("owned process group survived teardown")

    def stop(self):
        failures = []
        for process in reversed(self.processes):
            try:
                if process.poll() is None:
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
                for stream in (process.stdin, process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()
                self.check_stopped(process)
            except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                failures.append(error)
        if failures:
            raise RuntimeError("owned instance cleanup failed") from failures[0]



class Workspace:
    def __enter__(self):
        self.temp = tempfile.TemporaryDirectory(dir="/tmp", prefix="od-bi-")
        self.root = Path(self.temp.name).resolve()
        self.instances = []
        self.read_sentinel = self.root / "external-read-probe"
        self.read_sentinel.write_bytes(b"sandbox read probe")
        self.original_handlers = {}
        for signum in (signal.SIGINT, signal.SIGTERM):
            self.original_handlers[signum] = signal.signal(signum, self.interrupt)
        return self

    @staticmethod
    def interrupt(signum, _frame):
        raise InterruptedError(f"received signal {signum}")

    def instance(self, name):
        instance = Instance(self.root / name)
        self.instances.append(instance)
        instance.prove(self.read_sentinel, self.root / "external-write-probe")
        return instance

    def __exit__(self, *_exception):
        try:
            for signum in self.original_handlers:
                signal.signal(signum, signal.SIG_IGN)
            failures = []
            for instance in reversed(self.instances):
                try:
                    instance.stop()
                except RuntimeError as error:
                    failures.append(error)
            if failures:
                raise RuntimeError("owned workspace cleanup failed") from failures[0]
            executables = subprocess.check_output(["/bin/ps", "-axo", "comm="], text=True)
            if any(line.strip().startswith(f"{self.root}/") for line in executables.splitlines()):
                raise RuntimeError("workspace executable survived teardown")
        finally:
            try:
                self.temp.cleanup()
                if self.root.exists():
                    raise RuntimeError("test workspace survived teardown")
            finally:
                for signum, handler in self.original_handlers.items():
                    signal.signal(signum, handler)
