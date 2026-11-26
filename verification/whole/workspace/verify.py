"""Generate a native Cargo workspace with original files inaccessible; then inject reference tests."""
import argparse
import hashlib
import os
import sys
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


def linux_isolation(work):
    toolchain=Path(envtools.run("rustc","+1.97.1","--print","sysroot").strip()).resolve()
    registry=(Path(os.environ.get("CARGO_HOME",Path.home()/".cargo"))/"registry").resolve()
    probe=subprocess.run(["bwrap","--unshare-all","--ro-bind","/","/","/usr/bin/true"],capture_output=True)
    launcher=["bwrap"] if probe.returncode==0 else ["sudo","-n","bwrap"]
    args=launcher+["--unshare-net","--unshare-ipc","--unshare-uts","--unshare-pid","--die-with-parent","--new-session","--clearenv",
        "--ro-bind","/usr","/usr","--symlink","usr/bin","/bin","--symlink","usr/lib","/lib",
        "--proc","/proc","--dev","/dev","--perms","0755","--dir","/etc","--ro-bind","/etc/alternatives","/etc/alternatives",
        "--tmpfs","/tmp","--ro-bind",str(toolchain),"/toolchain","--perms","0755","--dir","/cargo",
        "--ro-bind",str(registry),"/cargo/registry","--bind",str(work),str(work),"--chdir",str(work),
        "--setenv","PATH","/toolchain/bin:/usr/bin","--setenv","CARGO_HOME","/cargo","--setenv","HOME",str(work),
        "--setenv","TMPDIR",str(work),"--setenv","CARGO_NET_OFFLINE","true","--setenv","LC_ALL","C"]
    if Path("/lib64").exists():args+=["--ro-bind","/lib64","/lib64"]
    if launcher[0]=="sudo":args+=["setpriv","--reuid",str(os.getuid()),"--regid",str(os.getgid()),"--clear-groups","--no-new-privs","--"]
    if envtools.run(*args,"readlink","/proc/self/ns/net").strip()==os.readlink("/proc/self/ns/net"):
        raise ValueError("network namespace was shared")
    envtools.run(*args,"grep","-Eq","^CapEff:[[:space:]]+0+$","/proc/self/status")
    return args, dict(os.environ)


def main():
    ap=argparse.ArgumentParser()
    for name in ("framework","original","receipt"):ap.add_argument(name,type=Path)
    ap.add_argument("--release", choices=("v1", "v2"), default="v1")
    args=ap.parse_args()
    combined = args.release == "v2"
    compiler = "911614cef63a84ecf840cb1228c8a944f1d54d48" if combined else FRAMEWORK
    original=args.original.resolve(); framework=args.framework.resolve()
    def pin(repo,expected):
        if envtools.run("git","rev-parse","HEAD",cwd=repo).strip()!=expected or envtools.run("git","status","--porcelain","--untracked-files=all",cwd=repo):
            raise ValueError(f"expected a clean pinned checkout: {repo}")
    pin(framework,compiler);pin(original,ORIGINAL)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve(); graph=work/"inputs";graph.mkdir()
        def stage(base,descriptor):
            orig=(base/descriptor["path"]).resolve()
            if not orig.is_relative_to(PORTABLE) or sha(orig.read_bytes())!=descriptor["sha256"]:
                raise ValueError("invalid closure input")
            target=graph/orig.relative_to(PORTABLE)
            target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(orig,target)
            return orig
        manifest=PORTABLE/("workspace-v2.json" if combined else "workspace.json")
        w=json.loads(manifest.read_bytes())
        shutil.copyfile(manifest,graph/manifest.name)
        stage(PORTABLE,w["cargoLock"])
        for member in w["members"].values():
            src=stage(PORTABLE,member["manifest"]); m=json.loads(src.read_bytes())
            for descriptor in [m["cargoLock"]]+[c[k] for c in m["components"] for k in ("graph","overlay")]:
                stage(src.parent,descriptor)
            for component in m["components"]:
                for descriptor in component.get("wireShape", {}).values():
                    stage(src.parent, descriptor)
        envtools.run("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",work/"compiler",cwd=framework)
        if sys.platform=="linux":
            sandbox,env=linux_isolation(work)
            platform="Linux "+os.uname().machine
        else:
            readable,env=envtools.isolation_environment(work,"aarch64-apple-darwin")
            sandbox=envtools.isolated(work,readable,original,framework,ROOT)
            platform="Darwin ARM64"
        target=work/"generated"
        def inside(*cmd,cwd=work):
            prefix=list(sandbox)
            if sys.platform=="linux":prefix[prefix.index("--chdir")+1]=str(cwd)
            return envtools.run(*prefix,*cmd,cwd=cwd,env=env)
        inside(work/"compiler/debug/oddlang","build","native-workspace","rust",graph/manifest.name,target)
        file=target/"workspace.lock"
        if file.read_bytes()!=(PORTABLE/f"workspace-{args.release}/workspace.lock").read_bytes():raise ValueError("generated workspace lock drift")
        receipt=json.loads(file.read_bytes())
        for name,r in receipt["members"].items():
            pkg=target/name; lock=(pkg/"package.lock").read_bytes()
            if sha(lock)!=r["standalonePackageLockSha256"] or sha((pkg/"Cargo.toml").read_bytes())!=r["workspaceCargoManifestSha256"]:raise ValueError("member receipt mismatch")
            generated={p.relative_to(pkg).as_posix():sha(p.read_bytes()) for p in (pkg/"src").rglob("*") if p.is_file()}
            if generated!=json.loads(lock)["generatedFiles"]:raise ValueError("member source inventory mismatch")
        if sha((target/"Cargo.lock").read_bytes())!=receipt["cargoLockSha256"]:raise ValueError("root Cargo.lock mismatch")
        probe=subprocess.run([*sandbox,"/bin/cat",original/"Cargo.toml"],env=env,cwd=work,capture_output=True)
        if probe.returncode==0 or not any(e in probe.stderr for e in (b"Operation not permitted",b"No such file or directory")):raise ValueError("reference source access")
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
        if combined:
            source = (original / "crates/marmish-stream/src/wire.rs").read_bytes()
            marker = b"#[cfg(test)]\nmod tests {"
            if source.count(marker) != 1:
                raise ValueError("unexpected upstream wire test content")
            suite = source[source.index(marker):]
            module_digests["wire"] = sha(suite)
            (stream / "src/original_wire_test_module.rs").write_bytes(suite)
            # The original tests refer to four private implementation constants.
            # These reference values belong only to the independent caller.
            with (stream / "src/lib.rs").open("a") as library:
                library.write("""
#[cfg(test)]
mod wire_caller {
    use crate::wire::*;
    use crate::codec::{Codec, EncodedBlock};
    use crate::{MAX_BLOCK, MAX_CONTENT, MAX_OFFSET, WINDOW_BYTES};
    const MAX_ARGV_BYTES: usize = 16384;
    const MAX_ARGV_ITEMS: usize = 128;
    const MAX_AGE_SECONDS: u64 = 604800;
    const FUTURE_SKEW_SECONDS: u64 = 60;
    include!("original_wire_test_module.rs");
}
""")
        output=inside("cargo","test","--workspace","--locked","--release","--offline",cwd=target)
        names=re.findall(r'^test ([\w:]+) \.\.\. ok$',output,re.M)
        expected_count = 25 if combined else 17
        if len(names)!=len(set(names)) or len(names)!=expected_count:raise ValueError(f"expected {expected_count} tests, saw {names}")
        args.receipt.write_text(json.dumps(dict(framework=compiler,original=ORIGINAL,workspaceLockSha256=sha(file.read_bytes()),
            qualification=platform+", release, source and network isolated",testModules=module_digests,tests=sorted(names)),indent=2)+"\n")


if __name__=="__main__":main()
