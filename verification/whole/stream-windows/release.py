"""Generate from a clean allowlist, run all original window unit tests and public traces."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
PORTABLE=ROOT/"oddlang/portable"
spec=importlib.util.spec_from_file_location("native_isolation",HERE.parent/"core-package/verify.py")
isolation=importlib.util.module_from_spec(spec)
spec.loader.exec_module(isolation)


def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    for name in ("original","oddlang","receipt"): ap.add_argument(name,type=Path)
    args=ap.parse_args()
    original=args.original.resolve()
    with tempfile.TemporaryDirectory(prefix="oddlang-window-") as tmp:
        work=Path(tmp).resolve();inputs=work/"inputs";inputs.mkdir()
        manifest=PORTABLE/"stream-windows-package.json"
        m=json.loads(manifest.read_bytes())
        for pin in [m["cargoLock"]]+[c[k] for c in m["components"] for k in ("graph","overlay")]:
            src=(PORTABLE/pin["path"]).resolve()
            if not src.is_relative_to(PORTABLE) or sha(src)!=pin["sha256"]:raise ValueError("invalid pin")
            dst=inputs/src.relative_to(PORTABLE);dst.parent.mkdir(exist_ok=True);shutil.copyfile(src,dst)
        shutil.copyfile(manifest,inputs/manifest.name)
        compiler=Path(shutil.copy2(args.oddlang,work/"oddlang"))
        readable,env=isolation.isolation_environment(work,"aarch64-apple-darwin")
        sandbox=isolation.isolated(work,readable,original,ROOT)
        def make(name):
            dest=work/name
            isolation.run(*sandbox,compiler,"build","native-package","rust",inputs/manifest.name,dest,cwd=work,env=env)
            return dest
        projected=make("generated")
        pkg=projected/"package.lock";expected_lock=PORTABLE/"stream-windows-v1/package.lock"
        if pkg.read_bytes()!=expected_lock.read_bytes():raise ValueError("package lock differs")
        for rel,expected in json.loads(pkg.read_bytes())["generatedFiles"].items():
            if sha(projected/rel)!=expected:raise ValueError(f"generated file differs: {rel}")
        isolation.run(*sandbox,"cargo","build","--release","--locked","--offline","--target-dir",work/"native-build",cwd=projected,env=env)
        source=original/"crates/marmish-stream/src/windows.rs"
        probe=subprocess.run([*sandbox,"/bin/cat",source],env=env,cwd=work,capture_output=True)
        if probe.returncode==0 or b"Operation not permitted" not in probe.stderr:raise ValueError("original read boundary")
        public=work/"public.json"
        isolation.run(sys.executable,"-B",HERE/"verify.py",original,projected,public)
        receipt=json.loads(public.read_bytes())
        data=source.read_bytes();marker=b"#[cfg(test)]\nmod tests {"
        if data.count(marker)!=1:raise ValueError("original test module changed")
        tests=data[data.index(marker):]
        which=next(i for i,c in enumerate(m["components"]) if c["kind"]=="sequence-journal")
        group=next(i for i,c in enumerate(m["components"]) if c["module"]=="windows")
        leaf=projected/f"src/group_{group}/__projection_{which}.rs"
        before=sha(leaf)
        with leaf.open("ab") as file:
            file.write(b"\n#[cfg(test)]\nuse crate::{Error, MAX_BLOCK, MAX_RECORDS, WINDOW_BYTES};\n")
            file.write(tests)
        base=isolation.run("cargo","+1.97.1","test","--release","--locked","--offline","-p","marmish-stream",
                           "--target-dir",work/"original-tests","--lib","windows::tests",cwd=original)
        other=isolation.run(*sandbox,"cargo","test","--release","--locked","--offline","--target-dir",work/"native-build","--lib","windows::",cwd=projected,env=env)
        def names(log):return sorted(re.findall(r'^test windows::(?:__projection_\d+::)?tests::([a-z_]+) \.\.\. ok$',log,re.M))
        if not (names(base)==names(other) and len(names(base))==5):raise ValueError("original suite differs")
        receipt["lexicalTests"]=names(other)
        receipt["windowTestModuleSha256"]=hashlib.sha256(tests).hexdigest()
        receipt["generatedModuleBeforeTestsSha256"]=before
        # A graph mutation changes only the outcome for conflicting retained data.
        selected=m["components"][which]["graph"]
        policy=inputs/selected["path"];g=json.loads(policy.read_bytes())
        matches=[c for op in g["operations"] for c in op["cases"] if c.get("reject","").endswith(":Conflict")]
        if len(matches)!=1:raise ValueError("expected one conflict rule")
        matches[0]["reject"]="urn:marmish:stream:error:OldRetry"
        policy.write_text(json.dumps(g));selected["sha256"]=sha(policy)
        (inputs/manifest.name).write_text(json.dumps(m))
        mutant=make("mutant")
        result=subprocess.run([sys.executable,"-B",HERE/"verify.py",original,mutant,work/"negative.json"],text=True,capture_output=True)
        if result.returncode==0 or "exact_retry_conflict_gap_eof_and_partial_delivery" not in result.stderr:
            raise ValueError("expected retry-vs-conflict control failed")
        receipt["mutation"]="changing the retained conflict to OldRetry fails the unchanged upstream public test"
        receipt["isolation"]="Seatbelt read allowlist and network deny during generation and release build"
        args.receipt.write_text(json.dumps(receipt,indent=2)+"\n")


if __name__=="__main__": main()
