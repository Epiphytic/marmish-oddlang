"""Compatibility entry points for callers of the native core-package verifier."""
import json
from functools import lru_cache
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
RUST = "1.97.1"
RUST_COMMIT = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"


def run(*args, cwd=None, env=None, fail=None, input_text=None):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, input=input_text)
    if fail is not None:
        if result.returncode == 0 or fail not in result.stderr:
            raise ValueError(f"negative control did not report {fail!r}: {args}\n{result.stdout}{result.stderr}")
    elif result.returncode:
        raise ValueError(f"exit {result.returncode}: {args}\n{result.stdout}{result.stderr}")
    return result.stdout


@lru_cache(maxsize=2)
def qualification_tool(name="core-package-inputs"):
    target = tempfile.TemporaryDirectory(prefix="core-package-tools-")
    run("cargo", f"+{RUST}", "build", "--quiet", "--locked", "--offline",
        "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
        "--bin", name, "--target-dir", target.name)
    return target, Path(target.name) / "debug" / name


def isolation_environment(work, triple):
    result = json.loads(run(qualification_tool()[1], "macos-environment", work, triple))
    return tuple(map(Path, result["readRoots"])), result["environment"]


def isolated(work, read_roots, *denied_paths):
    options = [value for path in read_roots for value in ("--read", path)]
    options.extend(value for path in denied_paths for value in ("--deny", path))
    policy = run(qualification_tool()[1], "macos-policy", work, *options)
    return Path("/usr/bin/sandbox-exec"), "-p", policy


if __name__ == "__main__":
    sys.exit(subprocess.call([qualification_tool()[1], "macos", *sys.argv[1:]]))
