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
from urllib.parse import urlsplit

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
PORTABLE=ROOT/"oddlang/portable"
ORIGINAL="af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK="25bca2d974a3a4d55fc05256d557f95e9f380ed1"
PROVISIONING_FRAMEWORK = "fa40d7fab0bd6308ed7b13ef2aa97206145ebd36"
UNIFIED_FRAMEWORK = "ab7171bb583f7e04c63896caeafaa476eb810d5a"
ENGINE_LIMITS_FRAMEWORK = "e31e5dcf305d419079af8033a706e8f61ec231ba"
REPORT_FRAMEWORK = "799a2467b0eccd9e5d902c5b6a81b1e289536b50"
PROVIDER_CACHE_KEYS = {
    "https://github.com/erskingardner/openmls.git": "openmls-daf8a12ad11cf9dc",
    "https://github.com/erskingardner/rust-nostr": "rust-nostr-86bd2a4a0ba8c210",
    "https://github.com/marmot-protocol/mdk": "mdk-7d5a3a2420b194f5",
    "https://github.com/rusqlite/rusqlite.git": "rusqlite-8e5e8204e489f2fd",
}
spec=importlib.util.spec_from_file_location("isolation",HERE.parent/"core-package/verify.py")
envtools=importlib.util.module_from_spec(spec)
spec.loader.exec_module(envtools)


def sha(data):return hashlib.sha256(data).hexdigest()


def external_packages(lock):
    entries = tomllib.loads(lock.read_text())["package"]
    return {(pkg["name"], pkg["version"], pkg["source"], pkg.get("checksum"))
            for pkg in entries if "source" in pkg}


def stage_provider_git_cache(lock, destination):
    source = (Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "git").resolve()
    checked_sources = {}
    for _, _, url, _ in sorted(external_packages(lock)):
        if not url.startswith("git+"):
            continue
        parsed = urlsplit(url.removeprefix("git+"))
        repository = parsed._replace(query="", fragment="").geturl()
        revision = parsed.fragment
        if parsed.query != f"rev={revision}" or not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise ValueError(f"provider requires a full pinned Git revision: {repository}")
        if repository in checked_sources and checked_sources[repository] != revision:
            raise ValueError(f"multiple Git revisions selected for {repository}")
        checked_sources[repository] = revision
    if checked_sources.keys() != PROVIDER_CACHE_KEYS.keys():
        raise ValueError("unexpected Git provider inventory")
    for repository, revision in checked_sources.items():
        name = PROVIDER_CACHE_KEYS[repository]
        cached = source / "db" / name
        pinned = destination / "db" / name
        pinned.parent.mkdir(parents=True, exist_ok=True)
        envtools.run("git", "init", "--quiet", "--bare", pinned)
        envtools.run("git", "-C", pinned, "fetch", "--quiet", "--depth=1",
                     "--no-tags", cached.as_uri(), revision)
        captured = envtools.run("git", "-C", pinned, "rev-parse", "FETCH_HEAD").strip()
        if captured != revision or (pinned / "shallow").read_text().strip() != revision:
            raise ValueError(f"Git provider did not stage exactly its locked commit: {repository}")
    return checked_sources


def linux_isolation(work, provider_cache=None):
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
    if provider_cache is not None:
        args += ["--bind", str(provider_cache), "/cargo/git"]
    if Path("/lib64").exists():args+=["--ro-bind","/lib64","/lib64"]
    if launcher[0]=="sudo":args+=["setpriv","--reuid",str(os.getuid()),"--regid",str(os.getgid()),"--clear-groups","--no-new-privs","--"]
    if envtools.run(*args,"readlink","/proc/self/ns/net").strip()==os.readlink("/proc/self/ns/net"):
        raise ValueError("network namespace was shared")
    envtools.run(*args,"grep","-Eq","^CapEff:[[:space:]]+0+$","/proc/self/status")
    return args, dict(os.environ)


def stage_workspace(manifest, graph):
    def stage(base,descriptor):
        orig=(base/descriptor["path"]).resolve()
        if not orig.is_relative_to(PORTABLE) or sha(orig.read_bytes())!=descriptor["sha256"]:
            raise ValueError("invalid closure input")
        target=graph/orig.relative_to(PORTABLE)
        target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(orig,target)
        return orig
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
    return w


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


def passing_tests(output):
    return re.findall(r"^test ([\w:]+) \.\.\. ok$", output, re.M)


def stage_copy_tests(original, stream):
    upstream = (original / "crates/marmish-stream/src/copy_protocol.rs").read_bytes()
    marker = b"#[cfg(test)]\nmod tests {"
    if upstream.count(marker) != 1:
        raise ValueError("unexpected original copy module")
    start = upstream.index(marker)
    suite = upstream[start:upstream.index(b"\nmod path_encoding {", start)]
    exact = (HERE.parent / "copy-frame/exact.rs").read_bytes()
    (stream / "src/original_copy_test_module.rs").write_bytes(suite)
    tests = stream / "tests"
    tests.mkdir()
    (tests / "exact_copy_frames.rs").write_bytes(exact)
    with (stream / "src/lib.rs").open("a") as lib:
        lib.write("""
#[cfg(test)]
mod copy_caller {
    use crate::{MAX_BLOCK, MAX_OFFSET, codec::Codec, copy_protocol::*};
    use sha2::{Digest, Sha256};
    const CONTEXT_DOMAIN: &[u8] = b"marmish-copy-context/1\\0";
    include!("original_copy_test_module.rs");
}
""")
    return sha(suite), sha(exact)


def profile_inspector(work, target, filename):
    dependencies = target / "target/release/deps"
    source = HERE.parent / "status" / filename
    inspector = work / source.stem
    command = ["rustc", "+1.97.1", "--edition", "2024", "-L", f"dependency={dependencies}"]
    for crate in ("rusqlite", "serde_json", "zeroize"):
        library, = dependencies.glob(f"lib{crate}-*.rlib")
        command.extend(("--extern", f"{crate}={library}"))
    envtools.run(*command, source, "-o", inspector)
    return inspector


def encrypted_profile_evidence(work, target, binaries, compiler):
    inspector = profile_inspector(work, target, "encrypted_store_inspect.rs")
    receipt = work / "encrypted-init.json"
    release = target / "target/release"
    envtools.run(sys.executable, "-B", HERE.parent / "status/encrypted_init_probe.py",
                 binaries / "marmish", release / "marmish", inspector, receipt,
                 "--framework-revision", compiler, "--generated-owner", release / "marmishd")
    return json.loads(receipt.read_bytes())


def configured_and_lifecycle_evidence(work, target, binaries, compiler, digests):
    release = target / "target/release"
    stores = profile_inspector(work, target, "configured_store_probe.rs")
    configured = work / "configured-init.json"
    envtools.run(sys.executable, "-B", HERE.parent / "status/configured_init_probe.py",
                 binaries / "marmish", release / "marmish", stores, configured,
                 "--framework-revision", compiler)
    inspector = work / "encrypted_store_inspect"
    lifecycle = work / "local-lifecycle.json"
    envtools.run(sys.executable, "-B", HERE.parent / "foreground-lifecycle/observe.py",
                 binaries, release, inspector, lifecycle, "--framework-revision", compiler)
    initialized = json.loads(configured.read_bytes())
    stopped = json.loads(lifecycle.read_bytes())
    if initialized["candidateBinarySha256"] != digests["marmish"] or stopped["candidateBinaries"] != digests:
        raise ValueError("configured and lifecycle checks exercised different generated executables")
    return initialized, stopped


def main():
    ap=argparse.ArgumentParser()
    for name in ("framework","original","receipt"):ap.add_argument(name,type=Path)
    ap.add_argument("--release", choices=("v1", "v2", "v3", "v4", "v5", "v6", "v7", "engine-limits"), default="v1")
    ap.add_argument("--original-bin-dir", type=Path)
    ap.add_argument("--original-cargo-target", type=Path)
    args=ap.parse_args()
    if args.release in ("v3", "v4", "v5", "v6", "v7", "engine-limits") and args.original_bin_dir is None:
        ap.error("this release requires --original-bin-dir for the mixed status replay")
    binaries = args.original_bin_dir.resolve() if args.original_bin_dir else None
    combined = args.release != "v1"
    engine_limits = args.release == "engine-limits"
    compiler = {"v1": FRAMEWORK, "v2": "911614cef63a84ecf840cb1228c8a944f1d54d48",
                "v3": "11a8b7038b37295729b9e43cfe9a917b9a688f14",
                "v4": "68dc2e327aae8242938df6379cf5568c39190d65", "v5": PROVISIONING_FRAMEWORK,
                "v6": UNIFIED_FRAMEWORK, "v7": REPORT_FRAMEWORK,
                "engine-limits": ENGINE_LIMITS_FRAMEWORK}[args.release]
    original=args.original.resolve(); framework=args.framework.resolve()
    def pin(repo,expected):
        if envtools.run("git","rev-parse","HEAD",cwd=repo).strip()!=expected or envtools.run("git","status","--porcelain","--untracked-files=all",cwd=repo):
            raise ValueError(f"expected a clean pinned checkout: {repo}")
    pin(framework,compiler);pin(original,ORIGINAL)
    companion = envtools.run("git", "rev-parse", "HEAD", cwd=ROOT).strip()
    provisioning = args.release in ("v5", "v6", "v7", "engine-limits")
    unified = args.release in ("v6", "v7", "engine-limits")
    report = args.release in ("v7", "engine-limits")
    if args.release in ("v4", "v5", "v6", "v7", "engine-limits"):
        pin(ROOT, companion)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve(); graph=work/"inputs";graph.mkdir()
        manifest=PORTABLE/(f"workspace-{args.release}.json" if combined else "workspace.json")
        w=stage_workspace(manifest,graph)
        if args.release in ("v4", "v5", "v6", "v7", "engine-limits"):
            reference_lock = original / "Cargo.lock"
            if sha(reference_lock.read_bytes()) != "9c5f7dde5c0f7d340e22dbefaa8a1c6ddb94c119011ba3e49ac8044c9b4cb0b3":
                raise ValueError("original dependency lock differs")
            available = external_packages(reference_lock)
            sealed_native = "native-foreground-lifecycle-v1/Cargo.lock" if unified else f"workspace-{args.release}/native-Cargo.lock"
            for descriptor in (w["cargoLock"]["path"], sealed_native):
                selected = external_packages(PORTABLE / descriptor)
                if not selected or not selected <= available:
                    raise ValueError(f"{args.release} closure differs from original: {descriptor}")
        provider_cache = work / "provider-git" if provisioning else None
        if provisioning:
            providers = stage_provider_git_cache(PORTABLE / w["cargoLock"]["path"], provider_cache)
        envtools.run("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",work/"compiler",cwd=framework)
        if sys.platform=="linux":
            sandbox,env=linux_isolation(work, provider_cache)
            platform="Linux "+os.uname().machine
        else:
            readable,env=envtools.isolation_environment(work,"aarch64-apple-darwin")
            if args.release in ("v4", "v5", "v6", "v7", "engine-limits"):
                env["CC"] = env["CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"]
                env["AR"] = envtools.run("xcrun", "--sdk", "macosx", "--find", "ar").strip()
                if provisioning:
                    sdk = ["xcrun", "--sdk", "macosx", "--find"]
                    make = Path(envtools.run(*sdk, "make").strip())
                    env.update({
                        "MACOSX_DEPLOYMENT_TARGET": "11.0", "XCRUN_CACHE_DISABLED": "1",
                        "RANLIB": envtools.run(*sdk, "ranlib").strip(),
                        "PATH": f"{make.parent}:{env['PATH']}",
                    })
                    developer = Path(env["DEVELOPER_DIR"])
                    xcode = next(parent for parent in developer.parents if parent.suffix == ".app")
                    readable = (*readable, xcode)
                    (Path(env["CARGO_HOME"]) / "git").symlink_to(provider_cache, target_is_directory=True)
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
            generated = {p.relative_to(pkg).as_posix(): sha(p.read_bytes())
                         for p in (pkg / "src").rglob("*") if p.is_file()}
            if report:
                allowed = {"Cargo.toml", "package.lock", "src"}
                if name == "marmish-native":
                    allowed.add("build.rs")
                    generated["build.rs"] = sha((pkg / "build.rs").read_bytes())
                if {p.name for p in pkg.iterdir()} != allowed:
                    raise ValueError(f"unexpected generated package paths: {name}")
            if generated != json.loads(lock)["generatedFiles"]:
                raise ValueError("member source inventory mismatch")
        if sha((target/"Cargo.lock").read_bytes())!=receipt["cargoLockSha256"]:raise ValueError("root Cargo.lock mismatch")
        if unified:
            if sha((target / "Cargo.toml").read_bytes()) != receipt["workspaceCargoManifestSha256"]:
                raise ValueError("workspace generated Cargo manifest mismatch")
            v5 = json.loads((PORTABLE / "workspace-v5/workspace.lock").read_bytes())
            for member in ("marmish-core", "marmish-stream"):
                if receipt["members"][member] != v5["members"][member]:
                    raise ValueError(f"v6 altered the sealed {member} from v5")
        inaccessible = [original / "Cargo.toml"]
        if args.release in ("v4", "v5", "v6", "v7", "engine-limits"):
            inaccessible += [binaries / "marmish", binaries / "marmishd", framework / "src/main.rs",
                             HERE.parent / "relay/owner-tls-macos-arm64.json"]
        if unified:
            inaccessible += [HERE.parent / "status/configured_init_probe.py",
                             HERE.parent / "foreground-lifecycle/observe.py"]
        if report:
            inaccessible += [HERE.parent / "build-report/observe.py",
                             original / "crates/marmish-native/src/native_manifest.rs"]
        if engine_limits:
            inaccessible += [HERE.parent / "engine-limits/observe.rs",
                             HERE.parent / "engine-limits/deny_darwin.c",
                             original / "crates/marmish-engine/src/platform/macos.rs"]
            if set(receipt["members"]) != {"marmish-core", "marmish-stream", "marmish-native", "marmish-engine"}:
                raise ValueError("unexpected engine workspace members")
            engine = json.loads((PORTABLE / "workspace-engine-limits/engine-package.lock").read_bytes())
            if (target/"marmish-engine/package.lock").read_bytes() != (PORTABLE/"workspace-engine-limits/engine-package.lock").read_bytes():
                raise ValueError("assembled engine differs from its standalone lock")
            if set(engine["generatedFiles"]) != {"src/lib.rs", "src/group_0/mod.rs", "src/group_0/__projection_0.rs"}:
                raise ValueError("unexpected scoped engine package inventory")
        for path in inaccessible:
            probe = subprocess.run([*sandbox, "/bin/cat", path], env=env, cwd=work, capture_output=True)
            if probe.returncode == 0 or not any(error in probe.stderr for error in (
                b"Operation not permitted", b"No such file or directory",
            )):
                raise ValueError(f"generation can read excluded input {path}")
        inside("cargo", "build", "--workspace", "--locked", "--release", "--offline", cwd=target)
        interfaces = v3_interfaces(work, target, original, binaries, compiler) if args.release in ("v3", "v4", "v5", "v6", "v7", "engine-limits") else {}
        if args.release in ("v4", "v5", "v6", "v7", "engine-limits"):
            mixed = work / "foreground.json"
            flags = ["--both-initializers"] if provisioning else []
            if unified:
                flags.append("--provider-terminal")
            envtools.run(sys.executable, "-B", HERE / "foreground_matrix.py", binaries, target, mixed, *flags)
            interfaces["foreground"] = json.loads(mixed.read_bytes())
            interfaces["companionRevision"] = companion
            interfaces["workspaceManifestSha256"] = sha(manifest.read_bytes())
            interfaces["cargoLockSha256"] = receipt["cargoLockSha256"]
        if provisioning:
            interfaces["gitProviders"] = providers
            interfaces["encryptedInit"] = encrypted_profile_evidence(work, target, binaries, compiler)
            if interfaces["foreground"]["generatedBinarySha256"] != interfaces["cliParser"]["generatedBinaries"]:
                raise ValueError("profile matrix exercised different generated executables")
        if unified:
            configured, lifecycle = configured_and_lifecycle_evidence(
                work, target, binaries, compiler, interfaces["cliParser"]["generatedBinaries"])
            interfaces["configuredInit"] = configured
            interfaces["localProviderLifecycle"] = lifecycle
        if report:
            observer = runpy.run_path(str(HERE.parent / "build-report/observe.py"))
            host = next(line.removeprefix("host: ") for line in
                        envtools.run("rustc", "+1.97.1", "-vV").splitlines() if line.startswith("host: "))
            reports = observer["observe"](original, PORTABLE, binaries, target / "target/release", host)
            if reports["generatedBinaries"] != interfaces["cliParser"]["generatedBinaries"]:
                raise ValueError("release-info exercised different generated executables")
            interfaces["releaseInfo"] = reports
        if engine_limits:
            observer = runpy.run_path(str(HERE.parent / "engine-limits/observe.py"))
            cache = args.original_cargo_target or work/"reference-engine"
            limits = observer["observe"](work, original, binaries, target, cache.resolve())
            if limits["generatedBinaries"] != interfaces["cliParser"]["generatedBinaries"]:
                raise ValueError("process limit startup exercised different generated executables")
            interfaces["engineProcessLimits"] = limits
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
        if provisioning:
            module_digests["copyProtocol"], module_digests["exactCopyFrames"] = stage_copy_tests(original, stream)
            reference_tests = envtools.run("cargo", "+1.97.1", "test", "--locked", "--release", "--offline",
                                           "--target-dir", work / "reference-library", "-p", "marmish-stream",
                                           "--lib", cwd=original)
            interfaces["originalStreamTests"] = sorted(passing_tests(reference_tests))
        output=inside("cargo","test","--workspace","--locked","--release","--offline",cwd=target)
        names = passing_tests(output)
        expected_count = 45 if provisioning else 25 if combined else 17
        if provisioning:
            generated_stream = [name.rsplit("::", 1)[1] for name in names if "::tests::" in name]
            reference_stream = [name.rsplit("::", 1)[1] for name in interfaces["originalStreamTests"]]
            if len(reference_stream) != 29 or sorted(generated_stream) != sorted(reference_stream):
                raise ValueError("the combined stream did not pass the complete set of upstream tests")
        if len(names)!=len(set(names)) or len(names)!=expected_count:raise ValueError(f"expected {expected_count} tests, saw {names}")
        args.receipt.write_text(json.dumps(dict(framework=compiler,original=ORIGINAL,workspaceLockSha256=sha(file.read_bytes()),
            qualification=platform+", release, source and network isolated",testModules=module_digests,tests=sorted(names),**interfaces),indent=2)+"\n")


if __name__=="__main__":main()
