"""Bind baseline runners and their CLI to one reviewed framework build."""
from contextlib import contextmanager
from pathlib import Path
import os
import subprocess
import tempfile

REVISION = "b653f1ada68791cd2fb70293bf227c5665850d1e"


def verify_checkout(root):
    def git(*args):
        return subprocess.check_output(["git", "-C", root, *args], text=True).strip()
    if git("rev-parse", "HEAD") != REVISION or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError(f"expected a clean Oddlang checkout at {REVISION}")


@contextmanager
def build_cli(root):
    verify_checkout(root)
    with tempfile.TemporaryDirectory(prefix="oddlang-baseline-build-") as target:
        env = {k: v for k, v in os.environ.items()
               if k not in ("RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                            "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
        subprocess.run(["cargo", "+1.97.1", "build", "--locked", "--offline",
                        "--bin", "oddlang", "--target-dir", target], cwd=root, env=env, check=True)
        yield Path(target) / "debug" / "oddlang"
