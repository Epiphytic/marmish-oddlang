"""Isolate the unchanged MDK tests after the macOS env-copy fixture fails."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def observe(binary):
    names = subprocess.check_output([binary, "--list", "--format", "terse"], text=True)
    results = {}
    for line in names.splitlines():
        if line.endswith(": test"):
            name = line.removesuffix(": test")
            run = subprocess.run([binary, "--exact", name], capture_output=True, timeout=60)
            results[name] = run.returncode
    with tempfile.TemporaryDirectory() as root:
        copied = shutil.copy("/usr/bin/env", Path(root) / "env")
        controls = {}
        for key, cmd in [("system", "/usr/bin/env"), ("copy", copied)]:
            proc = subprocess.run([cmd], env={}, capture_output=True, timeout=10)
            controls[key] = {"exit": proc.returncode, "stdoutBytes": len(proc.stdout),
                             "stderrBytes": len(proc.stderr)}
    return {"environmentExecutable": controls, "freshProcessPerTest": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("test_binary", type=Path)
    print(json.dumps(observe(os.fspath(parser.parse_args().test_binary.resolve())), indent=2))
