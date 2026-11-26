"""Acquire the v5 dependency closure in an empty Cargo home, rebuild offline and replay both CLIs."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from verify import HERE, PORTABLE, sha, stage_workspace

FRAMEWORK = "e2ec7338dcf141f95d2f1e24d9bee20cc1cec44b"


def run(*cmd, cwd=None, env=None, check=True):
    result = subprocess.run(cmd, cwd=cwd, env=env, text=True, capture_output=True)
    if check and result.returncode:
        raise RuntimeError(f"{cmd[0]} exited {result.returncode}\n{result.stderr}")
    return result


def qualify(framework):
    def git(*args):
        return run("git", *args, cwd=framework).stdout.strip()
    if git("rev-parse", "HEAD") != FRAMEWORK or git("status", "--porcelain"):
        raise ValueError("expected the pinned clean compiler checkout")
    sysroot = Path(run("rustc", "+1.97.1", "--print", "sysroot").stdout.strip())
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        inputs = root / "inputs"
        inputs.mkdir()
        manifest = PORTABLE / "workspace-v5.json"
        stage_workspace(manifest, inputs)
        inventory = {str(p.relative_to(inputs)): sha(p.read_bytes()) for p in sorted(inputs.rglob("*")) if p.is_file()}
        if any(Path(n).suffix not in {".json", ".lock", ".ttl"} for n in inventory):
            raise ValueError("unexpected product input")
        run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--locked", "--offline",
            "--target-dir", root / "compiler", cwd=framework)
        cli = root / "compiler/debug/oddlang"
        cache, home = root / "cargo", root / "home"
        cache.mkdir()
        home.mkdir()
        env = {"HOME":str(home), "CARGO_HOME":str(cache), "PATH":str(sysroot / "bin")+os.pathsep+os.defpath,
               "GIT_TERMINAL_PROMPT":"0", "LC_ALL":"C"}
        build = [cli, "build", "native-workspace", "rust", inputs / manifest.name]
        cold = run(*build, root / "cold", cwd=root, env=env, check=False)
        if cold.returncode == 0 or "offline" not in cold.stderr or (root / "cold").exists():
            raise ValueError("cold-cache offline control failed")
        # Use a second empty Cargo home so the acquisition has no cache state even from the control.
        cache = root / "release-cargo"
        cache.mkdir()
        initial_entries = len(list(cache.iterdir()))
        if initial_entries:
            raise ValueError("expected a new, empty Cargo home")
        env["CARGO_HOME"] = str(cache)
        run(*build, root / "connected", "--fetch-locked", cwd=root, env=env)
        env["CARGO_NET_OFFLINE"] = "true"
        product = root / "offline"
        run(*build, product, cwd=root, env=env)
        reference = PORTABLE / "workspace-v5/workspace.lock"
        pinned_workspace = json.loads(reference.read_bytes())
        for generated in (product, root / "connected"):
            if (generated / "workspace.lock").read_bytes() != reference.read_bytes():
                raise ValueError("generated root lock differs from the qualified workspace")
            if (sha((generated / "Cargo.toml").read_bytes()) != pinned_workspace["workspaceCargoManifestSha256"]
                    or sha((generated / "Cargo.lock").read_bytes()) != pinned_workspace["cargoLockSha256"]):
                raise ValueError("workspace Cargo inputs differ from release")
            for member, pins in pinned_workspace["members"].items():
                package = generated / member
                src = {str(f.relative_to(package)):sha(f.read_bytes()) for f in (package/"src").rglob("*") if f.is_file()}
                if sha((package / "Cargo.toml").read_bytes()) != pins["workspaceCargoManifestSha256"]:
                    raise ValueError("member Cargo manifest differs from release")
                lock = (package / "package.lock").read_bytes()
                if sha(lock) != pins["standalonePackageLockSha256"] or src != json.loads(lock)["generatedFiles"]:
                    raise ValueError("generated member differs from release")
        run("cargo", "build", "--workspace", "--release", "--locked", "--offline", cwd=product, env=env)
        release = product / "target/release"
        run(sys.executable, "-B", HERE.parent / "cli/replay.py", "verify", release, cwd=root, env=env)
        fixture = json.loads((HERE.parent / "cli/transcript.json").read_bytes())
        return {"framework":FRAMEWORK, "compilerSha256":sha(cli.read_bytes()),
                "rustc":run("rustc", "-vV", cwd=root, env=env).stdout.strip(),
                "workspaceLockSha256":sha(reference.read_bytes()), "inputs":inventory,
                "initialAcquisitionCacheFiles":initial_entries, "offlineColdRejected":True,
                "offlineReleaseBinaries":{name:sha((release/name).read_bytes()) for name in fixture},
                "parserCases":{name:len(rows) for name,rows in fixture.items()}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=Path)
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    report = qualify(args.framework.resolve())
    args.receipt.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
