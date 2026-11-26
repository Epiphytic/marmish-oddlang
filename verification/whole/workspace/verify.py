"""Generate a native Cargo workspace with original files inaccessible; then inject reference tests."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
PORTABLE=ROOT/"oddlang/portable"
ORIGINAL="af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK="25bca2d974a3a4d55fc05256d557f95e9f380ed1"
spec=importlib.util.spec_from_file_location("isolation",HERE.parent/"core-package/verify.py")
envtools=importlib.util.module_from_spec(spec)
spec.loader.exec_module(envtools)


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    for name in ("framework","original","receipt"):ap.add_argument(name,type=Path)
    args=ap.parse_args()
    original=args.original.resolve(); framework=args.framework.resolve()
    def pin(repo,expected):
        if envtools.run("git","rev-parse","HEAD",cwd=repo).strip()!=expected or envtools.run("git","status","--porcelain","--untracked-files=all",cwd=repo):
            raise ValueError(f"expected a clean pinned checkout: {repo}")
    pin(framework,FRAMEWORK);pin(original,ORIGINAL)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve(); graph=work/"inputs";graph.mkdir()
        def stage(base,descriptor):
            orig=(base/descriptor["path"]).resolve()
            if not orig.is_relative_to(PORTABLE) or sha(orig.read_bytes())!=descriptor["sha256"]:
                raise ValueError("invalid closure input")
            target=graph/orig.relative_to(PORTABLE)
            target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(orig,target)
            return orig
        manifest=PORTABLE/"workspace.json"
        w=json.loads(manifest.read_bytes())
        shutil.copyfile(manifest,graph/manifest.name)
        stage(PORTABLE,w["cargoLock"])
        for member in w["members"].values():
            src=stage(PORTABLE,member["manifest"]); m=json.loads(src.read_bytes())
            for descriptor in [m["cargoLock"]]+[c[k] for c in m["components"] for k in ("graph","overlay")]:
                stage(src.parent,descriptor)
        envtools.run("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",work/"compiler",cwd=framework)
        readable,env=envtools.isolation_environment(work,"aarch64-apple-darwin")
        sandbox=envtools.isolated(work,readable,original,framework,ROOT)
        target=work/"generated"
        def inside(*cmd,cwd=work):return envtools.run(*sandbox,*cmd,cwd=cwd,env=env)
        inside(work/"compiler/debug/oddlang","build","native-workspace","rust",graph/manifest.name,target)
        file=target/"workspace.lock"
        if file.read_bytes()!=(PORTABLE/"workspace-v1/workspace.lock").read_bytes():raise ValueError("generated workspace lock drift")
        receipt=json.loads(file.read_bytes())
        for name,r in receipt["members"].items():
            pkg=target/name; lock=(pkg/"package.lock").read_bytes()
            if sha(lock)!=r["standalonePackageLockSha256"] or sha((pkg/"Cargo.toml").read_bytes())!=r["workspaceCargoManifestSha256"]:raise ValueError("member receipt mismatch")
            generated={p.relative_to(pkg).as_posix():sha(p.read_bytes()) for p in (pkg/"src").rglob("*") if p.is_file()}
            if generated!=json.loads(lock)["generatedFiles"]:raise ValueError("member source inventory mismatch")
        if sha((target/"Cargo.lock").read_bytes())!=receipt["cargoLockSha256"]:raise ValueError("root Cargo.lock mismatch")
        probe=subprocess.run([*sandbox,"/bin/cat",original/"Cargo.toml"],env=env,cwd=work,capture_output=True)
        if probe.returncode==0 or b"Operation not permitted" not in probe.stderr:raise ValueError("reference source access")
        inside("cargo","build","--workspace","--locked","--release","--offline",cwd=target)
        core=target/"marmish-core";(core/"tests").mkdir()
        testfile=original/"crates/marmish-core/tests/contracts.rs";shutil.copyfile(testfile,core/"tests/contracts.rs")
        stream=target/"marmish-stream"
        stream_manifest=json.loads((PORTABLE/w["members"]["marmish-stream"]["manifest"]["path"]).read_bytes())
        module_digests={"coreContracts":sha(testfile.read_bytes())}
        for name,imports in (("codec","Error, MAX_BLOCK, MAX_OFFSET"),("windows","Error, MAX_BLOCK, MAX_RECORDS, WINDOW_BYTES")):
            i=next(i for i,c in enumerate(stream_manifest["components"]) if c["module"]==name)
            leaf=stream/f"src/group_{i}/__projection_{i}.rs"
            src=(original/f"crates/marmish-stream/src/{name}.rs").read_bytes();marker=b"#[cfg(test)]\nmod tests {"
            if src.count(marker)!=1:raise ValueError("unexpected upstream test content")
            suite=src[src.index(marker):]
            module_digests[name]=sha(suite)
            with leaf.open("ab") as f:
                f.write(f"\n#[cfg(test)]\nuse crate::{{{imports}}};\n".encode());f.write(suite)
        output=inside("cargo","test","--workspace","--locked","--release","--offline",cwd=target)
        names=re.findall(r'^test ([\w:]+) \.\.\. ok$',output,re.M)
        if len(names)!=len(set(names)) or len(names)!=17:raise ValueError(f"expected 17 tests, saw {names}")
        args.receipt.write_text(json.dumps(dict(framework=FRAMEWORK,original=ORIGINAL,workspaceLockSha256=sha(file.read_bytes()),
            qualification="Darwin ARM64, release, source and network isolated",testModules=module_digests,tests=sorted(names)),indent=2)+"\n")


if __name__=="__main__":main()
