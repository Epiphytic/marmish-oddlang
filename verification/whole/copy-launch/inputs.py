"""Shared build helpers for the remaining Python package verifiers."""
import hashlib
import importlib.util
import platform
import tempfile
import json
from pathlib import Path
import shutil
import subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
DATA=ROOT/"oddlang/portable/copy-launch-v1"
PIN="af54339d250290490d62d4475a77539b7e739071"
MANIFEST = DATA.parent / "stream-launch-package.json"


def write(path,obj):
    path.write_text(json.dumps(obj,indent=2,ensure_ascii=False)+"\n")
def read(path):
    return json.loads(path.read_bytes())
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def run(*args,cwd=None):
    return subprocess.run(args,cwd=cwd,capture_output=True,text=True,check=True).stdout.strip()
def archive_at(checkout, revision, destination):
    run("cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline", "--manifest-path",
        HERE.parent / "copy-manifest/Cargo.toml", "--bin", "copy-field-relations", "--",
        "archive", checkout, revision, destination)

def staged(work, manifest_path=MANIFEST):
    stage = work / "inputs"
    stage.mkdir()
    manifest = read(manifest_path)
    pins = [manifest["cargoLock"]] + [part[k] for part in manifest["components"] for k in ("graph","overlay")]
    for pin in pins:
        source = manifest_path.parent / pin["path"]
        assert sha(source) == pin["sha256"]
        dest = stage / pin["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
    shutil.copyfile(manifest_path, stage/manifest_path.name)
    assert not list(stage.rglob("*.rs"))
    return stage / manifest_path.name


def linux_project(cli, manifest, dest, original, caller, extra_sources=()):
    spec = importlib.util.spec_from_file_location("workspace_verifier", HERE.parent / "workspace/verify.py")
    workspace = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(workspace)
    with tempfile.TemporaryDirectory(prefix="odd-copy-", dir="/tmp") as directory:
        stage = Path(directory).resolve()
        shutil.copy2(cli, stage / "oddlang")
        inputs = stage / "inputs"
        shutil.copytree(manifest.parent, inputs)
        sandbox, env = workspace.linux_isolation(stage)
        for path in (original / "Cargo.toml", caller, *(repo / "Cargo.toml" for repo in extra_sources)):
            missing = subprocess.run([*sandbox, "/bin/cat", path], capture_output=True, env=env)
            if not missing.returncode or b"No such file or directory" not in missing.stderr:
                raise ValueError(f"reference file was visible inside the build: {path}")
        target = stage / "generated"
        workspace.envtools.run(*sandbox, stage / "oddlang", "build", "native-package", "rust",
                               inputs / manifest.name, target, cwd=stage, env=env)
        workspace.envtools.run(*sandbox, "cargo", "build", "--release", "--offline", "--locked",
                               "--manifest-path", target / "Cargo.toml", cwd=stage, env=env)
        shutil.move(str(target), dest)
    return True


def project(work, cli, manifest, dest, original, caller=HERE/"caller.rs"):
    if platform.system() == "Linux":
        return linux_project(cli, manifest, dest, original, caller)
    wrapper = []
    if platform.system() == "Darwin":
        profile = '(version 1)(allow default)(deny network*)' + ''.join(
            '(deny file-read* (subpath '+json.dumps(str(p.resolve()))+'))' for p in (ROOT,original))
        wrapper = ["/usr/bin/sandbox-exec", "-p", profile]
        for inaccessible in [caller,original/"Cargo.toml"]:
            control = subprocess.run([*wrapper,"/bin/cat",inaccessible],cwd=work,capture_output=True)
            assert control.returncode and b"Operation not permitted" in control.stderr
    run(*wrapper, cli, "build","native-package","rust",manifest,dest,cwd=work)
    run(*wrapper,"cargo","+1.97.1","build","--locked","--offline","--release",cwd=dest)
    return bool(wrapper)
