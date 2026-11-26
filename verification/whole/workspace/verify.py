"""Generate a native Cargo workspace with original files inaccessible; then inject reference tests."""
import argparse
import hashlib
import os
import sys
import importlib.util
import json
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import tempfile
import tomllib

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
PORTABLE=ROOT/"oddlang/portable"
ORIGINAL="af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK="25bca2d974a3a4d55fc05256d557f95e9f380ed1"
spec=importlib.util.spec_from_file_location("isolation",HERE.parent/"core-package/verify.py")
envtools=importlib.util.module_from_spec(spec)
spec.loader.exec_module(envtools)


def sha(data):return hashlib.sha256(data).hexdigest()


def registry(lock):
    entries = tomllib.loads(lock.read_text())["package"]
    return {(pkg["name"], pkg["version"], pkg["source"], pkg["checksum"])
            for pkg in entries if pkg.get("source", "").startswith("registry+")}


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


def v3_interfaces(work, target, original, binaries, compiler):
    lane = runpy.run_path(str(HERE.parent / "copy-field-relations/verify.py"))
    cases, expected = lane["corpus"]()
    for name, crate, lock in (
        ("original", original / "crates/marmish-stream", original / "Cargo.lock"),
        ("generated", target / "marmish-stream", target / "Cargo.lock"),
    ):
        executable = lane["consumer"](crate, work / ("relation-" + name), lock, lane["CALLER"])
        if lane["observe"](executable, cases) != expected:
            raise ValueError(f"{name} copy relations differ from portable oracle")
    runtime = work / "status.json"
    original_status = json.loads((HERE.parent / "status" / ("linux-arm64.json" if sys.platform == "linux" else "macos-arm64.json")).read_bytes())
    for name, digest in original_status["originalBinaries"].items():
        if sha((binaries / name).read_bytes()) != digest:
            raise ValueError(f"original executable digest mismatch: {name}")
    replay = HERE.parent / "cli/replay.py"
    fixture = replay.with_name("transcript.json")
    for bindir in (binaries, target / "target/release"):
        envtools.run(sys.executable, "-B", replay, "verify", bindir, "--fixture", fixture)
    parser_evidence = {
        "fixtureSha256": sha(fixture.read_bytes()),
        "casesPerRole": {role: len(rows) for role, rows in json.loads(fixture.read_bytes()).items()},
        "generatedBinaries": {role: sha((target / "target/release" / role).read_bytes())
                              for role in ("marmish", "marmishd")},
    }
    envtools.run(sys.executable, "-B", HERE.parent / "status/discovery_probe.py", binaries, runtime,
                 "--candidate", target / "target/release/marmish", "--framework-revision", compiler)
    status = json.loads(runtime.read_bytes())
    release = target / "target/release"
    libc, = (release / "deps").glob("liblibc-*.rlib")
    toolchain = Path(envtools.run("rustc", "+1.97.1", "--print", "sysroot").strip())
    process_limits = json.loads(envtools.run(
        sys.executable, "-B", HERE.parent / "suite/process_limits.py", original,
        "--rustc", toolchain / "bin/rustc", "--libc", libc,
        "--client", release / "marmish", "--daemon", release / "marmishd",
    ))
    if process_limits["executables"] != parser_evidence["generatedBinaries"]:
        raise ValueError("the process limits harness exercised different artifacts")
    return {
        "copyRelations": {"cases": len(cases), "callerSha256": sha(lane["CALLER"].read_bytes()),
                          "observationsSha256": sha(json.dumps(expected, separators=(",", ":")).encode())},
        "status": status,
        "cliParser": parser_evidence,
        "nativeProcessLimits": process_limits,
    }


def main():
    ap=argparse.ArgumentParser()
    for name in ("framework","original","receipt"):ap.add_argument(name,type=Path)
    ap.add_argument("--release", choices=("v1", "v2", "v3", "v4"), default="v1")
    ap.add_argument("--original-bin-dir", type=Path)
    args=ap.parse_args()
    if args.release in ("v3", "v4") and args.original_bin_dir is None:
        ap.error("this release requires --original-bin-dir for the mixed status replay")
    binaries = args.original_bin_dir.resolve() if args.original_bin_dir else None
    combined = args.release != "v1"
    compiler = {"v1": FRAMEWORK, "v2": "911614cef63a84ecf840cb1228c8a944f1d54d48",
                "v3": "11a8b7038b37295729b9e43cfe9a917b9a688f14",
                "v4": "68dc2e327aae8242938df6379cf5568c39190d65"}[args.release]
    original=args.original.resolve(); framework=args.framework.resolve()
    def pin(repo,expected):
        if envtools.run("git","rev-parse","HEAD",cwd=repo).strip()!=expected or envtools.run("git","status","--porcelain","--untracked-files=all",cwd=repo):
            raise ValueError(f"expected a clean pinned checkout: {repo}")
    pin(framework,compiler);pin(original,ORIGINAL)
    companion = envtools.run("git", "rev-parse", "HEAD", cwd=ROOT).strip()
    if args.release == "v4":
        pin(ROOT, companion)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve(); graph=work/"inputs";graph.mkdir()
        def stage(base,descriptor):
            orig=(base/descriptor["path"]).resolve()
            if not orig.is_relative_to(PORTABLE) or sha(orig.read_bytes())!=descriptor["sha256"]:
                raise ValueError("invalid closure input")
            target=graph/orig.relative_to(PORTABLE)
            target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(orig,target)
            return orig
        manifest=PORTABLE/(f"workspace-{args.release}.json" if combined else "workspace.json")
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
        if args.release == "v4":
            reference_lock = original / "Cargo.lock"
            if sha(reference_lock.read_bytes()) != "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3":
                raise ValueError("original dependency lock differs")
            available = registry(reference_lock)
            for descriptor in ("Cargo.lock", "native-Cargo.lock"):
                selected = registry(PORTABLE / "workspace-v4" / descriptor)
                if not selected or not selected <= available:
                    raise ValueError(f"v4 closure differs from original: {descriptor}")
        envtools.run("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",work/"compiler",cwd=framework)
        if sys.platform=="linux":
            sandbox,env=linux_isolation(work)
            platform="Linux "+os.uname().machine
        else:
            readable,env=envtools.isolation_environment(work,"aarch64-apple-darwin")
            if args.release == "v4":
                env["CC"] = env["CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"]
                env["AR"] = envtools.run("xcrun", "--sdk", "macosx", "--find", "ar").strip()
                sandbox = envtools.isolated(work, readable, original, framework, ROOT, binaries)
            else:
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
        inaccessible = [original / "Cargo.toml"]
        if args.release == "v4":
            inaccessible += [binaries / "marmish", binaries / "marmishd", framework / "src/main.rs",
                             HERE.parent / "relay/owner-tls-macos-arm64.json"]
        for path in inaccessible:
            probe = subprocess.run([*sandbox, "/bin/cat", path], env=env, cwd=work, capture_output=True)
            if probe.returncode == 0 or not any(error in probe.stderr for error in (
                b"Operation not permitted", b"No such file or directory",
            )):
                raise ValueError(f"generation can read excluded input {path}")
        inside("cargo", "build", "--workspace", "--locked", "--release", "--offline", cwd=target)
        interfaces = v3_interfaces(work, target, original, binaries, compiler) if args.release in ("v3", "v4") else {}
        if args.release == "v4":
            mixed = work / "foreground.json"
            envtools.run(sys.executable, "-B", HERE / "foreground_matrix.py", binaries, target, mixed)
            interfaces["foreground"] = json.loads(mixed.read_bytes())
            interfaces["companionRevision"] = companion
            interfaces["workspaceManifestSha256"] = sha(manifest.read_bytes())
            interfaces["cargoLockSha256"] = receipt["cargoLockSha256"]
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
            qualification=platform+", release, source and network isolated",testModules=module_digests,tests=sorted(names),**interfaces),indent=2)+"\n")


if __name__=="__main__":main()
