"""Compare the public vault helper mode type and a source-order mutation."""
import os
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent
FRAMEWORK = "18ea58006de4482c0ebc61de9e800edc78eba987"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
JOURNAL = runpy.run_path(str(HERE.parent / "journal-records/verify.py"))
WORKSPACE = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
CORE = runpy.run_path(str(HERE.parent / "core-package/verify.py"))
run = JOURNAL["run"]


def reject_local_cargo_config(directory):
    for base in (directory, *directory.parents):
        for name in ("config", "config.toml"):
            config = base / ".cargo" / name
            if config.is_file():
                raise ValueError(f"unexpected Cargo configuration: {config}")


def host_build_env(cargo_home):
    registry_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    cargo_home.mkdir()
    for name in ("registry", "git"):
        cache = registry_home / name
        if cache.is_dir():
            (cargo_home / name).symlink_to(cache.resolve(), target_is_directory=True)
    env = JOURNAL["cargo_env"]()
    env = {key: value for key, value in env.items()
           if not key.startswith(("CARGO_", "RUSTC", "RUSTDOC", "RUSTFLAGS"))}
    env.update(CARGO_HOME=str(cargo_home), CARGO_NET_OFFLINE="true")
    return env


def check_inputs(args):
    for path, expected in ((args.framework, FRAMEWORK), (args.original, ORIGINAL)):
        reject_local_cargo_config(path)
        revision = run("git", "rev-parse", "HEAD", cwd=path).decode().strip()
        dirty = run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)
        if revision != expected or dirty:
            raise ValueError(f"expected a clean checkout at {expected}: {path}")


if __name__ == "__main__":
    build_dir, tool = CORE["qualification_tool"]("qualify-vault-mode")
    with build_dir:
        sys.exit(subprocess.call([tool, *sys.argv[1:]]))
