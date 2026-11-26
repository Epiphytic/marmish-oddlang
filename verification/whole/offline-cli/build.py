"""Build the base or successor binaries from an isolated graph closure."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

from compose import BASE, HERE, ROOT, WORKSPACE, sha


def build(compiler, framework, original, output, target, successor):
    lane = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
    manifest = WORKSPACE if successor else BASE
    primary_lock = manifest.with_suffix("") / "workspace.lock"
    revision = subprocess.check_output(["git", "-C", framework, "rev-parse", "HEAD"], text=True).strip()
    subprocess.run(["git", "-C", framework, "diff", "--quiet", "HEAD", "--", "src", "Cargo.toml", "Cargo.lock"], check=True)
    output = output.resolve()
    if output.exists():
        raise ValueError("expected an absent product directory")
    with tempfile.TemporaryDirectory(prefix="offline-cli-graph-") as temp:
        root = Path(temp).resolve()
        inputs = root / "inputs"
        inputs.mkdir()
        spec = lane["stage_workspace"](manifest, inputs)
        isolated_compiler = root / "oddlang"
        shutil.copy2(compiler, isolated_compiler)
        deny = [framework.resolve(), original.resolve(), ROOT.parents[1]]
        sandbox = ["/usr/bin/sandbox-exec", "-p", "(version 1) (allow default) (deny network*)"
                   + "".join(f" (deny file-read* (subpath {json.dumps(str(path))}))" for path in deny)]
        for path in (deny[0] / "Cargo.toml", deny[1] / "Cargo.toml", manifest):
            if not path.is_file() or subprocess.run(
                    [*sandbox, "/bin/cat", path], capture_output=True).returncode == 0:
                raise ValueError(f"source isolation did not exclude {path}")
        subprocess.run([*sandbox, isolated_compiler, "build", "native-workspace", "rust",
                        inputs / manifest.name, output],
                       env={**os.environ, "RUSTUP_TOOLCHAIN": "1.97.1"}, check=True)
        lock = (output / "workspace.lock").read_bytes()
        if lock != primary_lock.read_bytes():
            raise ValueError("generated workspace lock differs")
        lane["verify_generated_tree"](output, json.loads(lock), True, True)
        previous = json.loads((BASE.with_suffix("") / "workspace.lock").read_bytes())
        current = json.loads(lock)
        if set(current["members"]) != set(spec["members"]) or current["cargoLockSha256"] != previous["cargoLockSha256"]:
            raise ValueError("workspace member set or Cargo dependency closure changed")
        for name, member in previous["members"].items():
            if (name != "marmish-native" or not successor) and current["members"][name] != member:
                raise ValueError(f"unrelated sealed workspace member changed: {name}")
        subprocess.run([*sandbox, "cargo", "+1.97.1", "build", "--locked", "--offline", "--release",
                        "-p", "marmish-native", "--bins", "--target-dir", target.resolve()],
                       cwd=output, check=True)
        lane["verify_generated_tree"](output, current, True, True)
    return {"frameworkRevision": revision, "compilerSha256": sha(compiler.read_bytes()), "workspaceLockSha256": sha(lock),
            "workspaceManifestSha256": sha(manifest.read_bytes()),
            "binarySha256": {name: sha((target / "release" / name).read_bytes()) for name in ("marmish", "marmishd")}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "framework", "original", "output", "target", "receipt"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--successor", action="store_true")
    args = parser.parse_args()
    result = build(args.compiler, args.framework, args.original, args.output, args.target,
                   args.successor)
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
