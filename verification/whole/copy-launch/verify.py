"""Reproduce the portable Launch release, compare both native APIs, then falsify its guard order."""
import argparse
from hashlib import sha256
import json
import importlib.util
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

from inputs import DATA, HERE, PIN, ROOT, consumer, derive, observe, read, run, sha, write

FRAMEWORK = "5f4cf958b7b4f8da8f8309f87734c14a8309c7d7"
MANIFEST = DATA.parent / "stream-launch-package.json"


def compact(value):
    return json.dumps(value, separators=(",", ":"))


def corpus(work):
    raw = subprocess.check_output(["cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline",
                                   "--manifest-path", HERE.parent / "copy-manifest/Cargo.toml",
                                   "--target-dir", work / "corpus-tools", "--bin", "launch-data-cases"])
    return json.loads(raw)


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","receipt"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    original, fw = args.original.resolve(), args.framework.resolve()
    assert run("git","rev-parse","HEAD",cwd=fw) == FRAMEWORK
    assert not run("git","diff","HEAD","--",cwd=fw)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=fw)
        cli=work/"tools/debug/oddlang"
        manifest=staged(work)
        dest=work/"generated"
        sandbox=project(work,cli,manifest,dest,original)
        assert (dest/"package.lock").read_bytes() == (DATA/"package.lock").read_bytes()
        old=consumer(original/"crates/marmish-stream",work/"reference",original/"Cargo.lock")
        new=consumer(dest,work/"native",dest/"Cargo.lock")
        table=derive(original,cli,old,work/"recovery")
        for name in ("launch.json","launch-rust.json","enums.json","enums-rust.json"):
            assert (work/"recovery"/name).read_bytes()==(DATA/name).read_bytes(),name
        assert (work/"recovery/candidate/observation.json").read_bytes()==(DATA/"observation.json").read_bytes()
        assert observe(new,[dict(operation="enums")])==[table]
        cases=corpus(work)
        creates,expected,decodes=cases["creates"],cases["expected"],cases["decodes"]
        assert observe(old,creates)==expected==observe(new,creates)
        decoded=observe(old,decodes)
        assert decoded==observe(new,decodes)
        graph_path=manifest.parent/"copy-launch-v1/launch.json"
        graph=read(graph_path)
        graph["guards"].append(graph["guards"].pop(0))
        write(graph_path,graph)
        mutated=read(manifest)
        mutated["components"][-1]["graph"]["sha256"]=sha(graph_path)
        write(manifest,mutated)
        mutant=work/"mutant"
        project(work,cli,manifest,mutant,original)
        negative=consumer(mutant,work/"mutant-observer",mutant/"Cargo.lock")
        req,baseline=next((r,row) for r,row in zip(creates,expected) if r["version"]==0 and not r["source"])
        got,=observe(negative,[req])
        assert got["valid"]["Err"][0]=='Invalid("copy_path")' and baseline["valid"]["Err"][0]=='Invalid("copy_version")'
        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
              originalAndCompanionReadDenied=sandbox,packageLockSha256=sha(DATA/"package.lock"),
              callerSha256=sha(HERE/"caller.rs"),nativeEnumByteOutcomes=5*256,constructCases=len(creates),
              exactDecodeCases=len(decodes),negativeControl="version guard moved last selects copy_path instead of copy_version",
              observationsSha256=sha256(compact([table,expected,decoded]).encode()).hexdigest()))
        print("Qualified", len(creates), "native constructions,", len(decodes), "decodes, and all 1,280 enum inputs.")
if __name__ == "__main__":
    main()
