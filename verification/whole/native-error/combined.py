"""Build the combined workspace and replay the pinned CLI and library observations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

from verify import HERE, PORTABLE, native_probe

FRAMEWORK = "da0ea5dedd1adfa8513b7f3f8d536713958bcecb"
MANIFEST = PORTABLE / "workspace-v7-native-error.json"
WORKSPACE = HERE.parent / "workspace"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(*args, **kwargs):
    return subprocess.check_output(args, **kwargs)


def qualify(framework, cache):
    if run("git", "rev-parse", "HEAD", cwd=framework).decode().strip() != FRAMEWORK or run(
        "git", "status", "--porcelain", cwd=framework
    ):
        raise ValueError("expected the clean pinned framework")
    version = run("rustc", "+1.97.1", "-vV").decode()
    target, = [s.removeprefix("host: ") for s in version.splitlines() if s.startswith("host: ")]
    reference = json.loads((WORKSPACE / f"v7-{target}.json").read_bytes())["releaseInfo"]
    if set(reference["cases"]) != {"marmish", "marmishd"} or any(
        len(cases) != 16 for cases in reference["cases"].values()
    ):
        raise ValueError("incomplete v7 client and daemon reference")
    error_reference = HERE / "darwin-arm64/baseline.txt"
    error_receipt = json.loads((error_reference.parent / "receipt.json").read_bytes())
    if (sha(error_reference.read_bytes()) != error_receipt["profiles"]["baseline"]["originalAndGeneratedOutputSha256"]
            or sha((HERE / "probe.rs").read_bytes()) != error_receipt["callerSha256"]):
        raise ValueError("native error reference or caller differs from its reviewed receipt")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        inputs = root / "inputs"
        inputs.mkdir()
        runpy.run_path(str(WORKSPACE / "verify.py"))["stage_workspace"](MANIFEST, inputs)
        files = sorted(p for p in inputs.rglob("*") if p.is_file())
        if any(p.suffix not in {".json", ".ttl", ".lock"} for p in files):
            raise ValueError("unexpected build input")
        run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--target-dir", cache / "compiler",
            "--offline", "--locked", cwd=framework)
        product = root / "generated"
        env = dict(os.environ, RUSTUP_TOOLCHAIN="1.97.1", CARGO_NET_OFFLINE="true")
        run(cache / "compiler/debug/oddlang", "build", "native-workspace", "rust",
            inputs / MANIFEST.name, product, cwd=root, env=env)
        run("cargo", "+1.97.1", "build", "--release", "--workspace", "--target-dir", cache / "app",
            "--locked", "--offline", cwd=product, env=env)
        expected = error_reference.read_bytes()
        observed = native_probe(product, cache / "app", root / "caller")
        if observed != expected:
            raise ValueError("independent native error caller output changed")
        runtime = root / "runtime"
        runtime.mkdir()
        cli_env = dict(HOME=str(runtime), TMPDIR=str(runtime), LANG="C", PATH=os.defpath,
                       TARGET="override", CARGO_PKG_VERSION="override", MARMISH_BUILD_COMMIT="override",
                       MARMISH_BUILD_TARGET="override")
        binaries = {}
        for role, cases in reference["cases"].items():
            binary = cache / "app/release" / role
            for case in cases:
                call = subprocess.run([binary, *case["argv"]], cwd=runtime, env=cli_env,
                                      capture_output=True, input=b"", timeout=30)
                if (call.returncode, call.stdout, call.stderr) != (
                    case["exit"], case["stdout"].encode(), case["stderr"].encode()
                ):
                    raise ValueError(f"{role} diverged for {case['argv']}")
            binaries[role] = {"cases": len(cases), "sha256": sha(binary.read_bytes())}
        if list(runtime.iterdir()):
            raise ValueError("release-info created runtime files")
        return dict(framework=FRAMEWORK, rustc=version.splitlines(), manifestSha256=sha(MANIFEST.read_bytes()),
                    workspaceLockSha256=sha((product / "workspace.lock").read_bytes()), binaries=binaries,
                    referenceBinaries=reference["originalBinaries"], callerSha256=sha((HERE / "probe.rs").read_bytes()),
                    callerOutputSha256=sha(observed), callerRows=len(observed.splitlines()),
                    stagedInputs={str(p.relative_to(inputs)): sha(p.read_bytes()) for p in files})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "cache", "receipt"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    result = qualify(args.framework.resolve(), args.cache.resolve())
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
