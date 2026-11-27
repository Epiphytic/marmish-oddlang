"""Bind selected upstream harnesses to a sealed workspace and count uncovered named tests."""
import argparse
import atexit
from functools import lru_cache
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tomllib
from typing import NamedTuple
import tempfile


HERE = Path(__file__).resolve().parent
PIN = "af54339d250290490d62d4475a77539b7e739071"
WHOLE = HERE.parent
CORE = "marmish-core/test/contracts"
STREAM = "marmish-stream/lib/marmish_stream"
NATIVE = "marmish-native/test/process_limits"
FRAMEWORK = "7ed1c29acde2cc4cfdc58c2489935e703470ae70"
MANIFEST = HERE.parents[2] / "oddlang/portable/workspace-v7-vault-engine.json"
PUBLIC_DATA = "canonical-public-data"
MDK_DENIED_SOURCES = ("lib.rs", "tests.rs")


class Candidate(NamedTuple):
    manifest: Path
    compiler: str
    suite: str


CANDIDATES = {
    "v7": Candidate(MANIFEST, FRAMEWORK, "v7"),
    PUBLIC_DATA: Candidate(MANIFEST.with_name("workspace-v7-canonical-public-data.json"),
                           "900cfa517d0136fa80c2b5e92ccf9b1a3b2a71ab", "manifest"),
    "canonical-mdk": Candidate(MANIFEST.with_name("workspace-v7-canonical-mdk-test-seam.json"),
                               "48bc6754dcbe12cfa953b94ca8771001a1ce72ec", "manifest-mdk"),
    "canonical": Candidate(MANIFEST.with_name("workspace-v7-canonical-offline.json"),
                           "8a20ea283d0d43da42f054a7c7774eb9e6a20da7", "manifest"),
    "manifest": Candidate(MANIFEST.with_name("workspace-v7-converged-release-manifest.json"),
                          "da1b0033ae9345c2d8288b4164d4fd1dbf5bcade", "manifest"),
    "seven": Candidate(MANIFEST.with_name("workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"),
                       "59f947dafdb4cf3055e35216857472ada895a76a", "seven"),
    "recovered": Candidate(MANIFEST.with_name("workspace-v7-recovered-vault-engine.json"),
                           "6e2d1ef00f286b6132b84e27a61805c659dfd437", "recovered"),
}


def run(*cmd, **kwargs):
    return subprocess.check_output(cmd, **kwargs)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pin(root, revision):
    if run("git", "rev-parse", "HEAD", cwd=root).decode().strip() != revision or run(
        "git", "status", "--porcelain", cwd=root
    ):
        raise ValueError(f"expected clean {revision} at {root}")


@lru_cache(maxsize=1)
def suite_tools():
    target = tempfile.TemporaryDirectory(prefix="odd-suite-tools-")
    try:
        run("cargo", "+1.97.1", "build", "--quiet", "--locked", "--offline",
            "--manifest-path", HERE / "Cargo.toml", "--target-dir", target.name,
            "--bin", "suite-selection", "--bin", "reconcile-suite")
    except BaseException:
        target.cleanup()
        raise
    atexit.register(target.cleanup)
    return Path(target.name) / "debug"


def suite_check(tool, *args, data=None):
    result = subprocess.run([suite_tools() / tool, *args], capture_output=True,
                            input=json.dumps(data).encode() if data is not None else b"")
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return json.loads(result.stdout)


def default_baseline(host):
    return suite_check("suite-selection", "baseline", host)


def check_default_baseline(host, harnesses):
    return suite_check("suite-selection", "check-baseline", host, data=harnesses)


def check_registered_names(harness, listing, expected):
    return suite_check("suite-selection", "registered", harness, data=[listing, expected])


def reconcile(inventory, passed):
    return tuple(suite_check("reconcile-suite", data=[inventory, passed]))


def qualify(original, framework, compiler_cache, reference_target, candidate="v7"):
    manifest, framework_pin, suite_kind = CANDIDATES[candidate]
    pin(original, PIN)
    pin(framework, framework_pin)
    version = run("rustc", "+1.97.1", "-vV").decode()
    host, = [line[6:] for line in version.splitlines() if line.startswith("host: ")]
    baseline = default_baseline(host) if candidate == PUBLIC_DATA else None
    suite = HERE / f"named-release-{host}.json"
    inventory = json.loads(suite.read_bytes())
    if inventory["upstream"] != PIN or inventory["rustc"] != version.strip() or inventory["selection"] != "native release workspace, default features":
        raise ValueError("upstream inventory profile differs from candidate")
    if sha((original/"Cargo.lock").read_bytes()) != inventory["cargoLockSha256"]:
        raise ValueError("upstream Cargo.lock changed")
    manifest_digest = sha(manifest.read_bytes())
    expected_lock = (manifest.with_suffix("")/"workspace.lock").read_bytes()
    prior = json.loads((WHOLE/"workspace"/f"v7-{host}.json").read_bytes())
    assembly = runpy.run_path(str(WHOLE/"workspace/verify.py"))
    run("cargo", "+1.96.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", compiler_cache, cwd=framework)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        inputs, product = work/"inputs", work/"product"
        inputs.mkdir()
        workspace = assembly["stage_workspace"](manifest, inputs)
        compiler = compiler_cache/"debug/oddlang"
        if host == "aarch64-apple-darwin":
            sandbox = runpy.run_path(str(WHOLE/"unified-vault/verify.py"))["sandbox"]
            prefix, cli, _ = sandbox(work, original, compiler, framework, reference_target)
            prefix += ["RUSTUP_TOOLCHAIN=1.97.1"]
            env = os.environ
        elif host == "aarch64-unknown-linux-gnu":
            provider_cache = work/"provider-git"
            assembly["stage_provider_git_cache"](inputs/workspace["cargoLock"]["path"], provider_cache)
            prefix, env = assembly["linux_isolation"](work, provider_cache)
            cli = work/"oddlang"
            shutil.copy2(compiler, cli)
            for forbidden in (original/"Cargo.toml", HERE/"inventory.json", framework/"src/lib.rs"):
                denied = subprocess.run([*prefix, "/bin/cat", forbidden], env=env, capture_output=True)
                if not forbidden.is_file() or not denied.returncode or b"No such file" not in denied.stderr:
                    raise ValueError(f"failed isolation control {forbidden}")
        else:
            raise ValueError(f"unsupported build isolation host {host}")

        def inside(*cmd, cwd):
            args = list(prefix)
            if host.endswith("linux-gnu"):
                args[args.index("--chdir")+1] = str(cwd)
            return run(*args, *cmd, cwd=cwd, env=env)

        if suite_kind == "manifest-mdk":
            for name in MDK_DENIED_SOURCES:
                forbidden = original / "crates/marmish-mdk/src" / name
                denied = subprocess.run([*prefix, "/bin/cat", forbidden], env=env, capture_output=True)
                if (not forbidden.is_file() or denied.returncode != 1 or denied.stdout
                        or not any(reason in denied.stderr for reason in
                                   (b"No such file", b"Operation not permitted"))):
                    raise ValueError(f"failed MDK source isolation control for {name}")
        inside(cli, "build", "native-workspace", "rust", inputs/manifest.name, product, cwd=work)
        lock = (product/"workspace.lock").read_bytes()
        if lock != expected_lock:
            raise ValueError("generated workspace differs from the published candidate")
        seal = json.loads(lock)
        assembly["verify_generated_tree"](product, seal, True, True)

        def snapshot():
            return {path.relative_to(product).as_posix(): path.read_bytes()
                    for member in [*seal["members"], "tools"] for path in (product/member).rglob("*")
                    if path.is_file()}

        pristine = snapshot()
        include_mdk = suite_kind == "manifest-mdk"
        include_manifest = suite_kind in {"manifest", "manifest-mdk"}
        include_reader = suite_kind == "seven" or include_manifest
        if include_mdk:
            prior_lock = CANDIDATES["canonical"].manifest.with_suffix("") / "workspace.lock"
            checked = inside("cargo", "tree", "--locked", "--offline", "-p", "marmish-mdk",
                             "-e", "normal", "--prefix", "none", "--format", "{p}", cwd=product).decode()
            suite_check("suite-selection", "check-mdk-seam", prior_lock, product / "workspace.lock",
                       product, data=checked)
        targets = ["--workspace","--lib","--bins"] if include_reader else ["-p","marmish-native","--bins"]
        built_libraries = {}
        if include_reader:
            artifacts = inside("cargo","build","--locked","--offline","--release",*targets,"--message-format=json",cwd=product)
            for line in artifacts.splitlines():
                event = json.loads(line)
                if event.get("reason") != "compiler-artifact" or event["target"]["kind"] != ["lib"]:
                    continue
                name = event["target"]["name"].replace("_","-")
                if name not in seal["members"]:
                    continue
                manifest_path = Path(event["manifest_path"])
                if manifest_path != product/name/"Cargo.toml" or name in built_libraries:
                    raise ValueError(f"wrong compiled workspace library identity: {name}")
                library, = [Path(f) for f in event["filenames"] if f.endswith(".rlib")]
                if not library.is_relative_to(product/"target/release"):
                    raise ValueError(f"unexpected candidate artifact location: {name}")
                built_libraries[name] = sha(library.read_bytes())
            if set(built_libraries) != set(seal["members"]):
                raise ValueError("Cargo did not build every sealed library")
        else:
            inside("cargo","build","--locked","--offline","--release",*targets,cwd=product)
        release = product/"target/release"
        binary_hashes = {role: sha((release/role).read_bytes()) for role in ("marmish","marmishd")}
        pin(original, PIN)
        assembly["verify_generated_tree"](product, seal, True, True)
        modules = assembly["stage_library_tests"](product, original, workspace, True, True)
        modules.pop("exactCopyFrames")
        if modules != {k:v for k,v in prior["testModules"].items() if k != "exactCopyFrames"}:
            raise ValueError("upstream test module identity changed")
        stream = workspace["members"]["marmish-stream"]["manifest"]["path"]
        components = json.loads((assembly["PORTABLE"]/stream).read_bytes())["components"]
        appended = {"marmish-stream/src/lib.rs"} | {
            f"marmish-stream/src/group_{i}/__projection_{i}.rs"
            for i, component in enumerate(components) if component["module"] in {"codec","windows"}}
        injected = {"marmish-core/tests/contracts.rs","marmish-stream/src/original_wire_test_module.rs",
                    "marmish-stream/src/original_copy_test_module.rs","marmish-stream/tests/exact_copy_frames.rs"}
        additional = {}
        if suite_kind != "v7":
            helpers = runpy.run_path(str(WHOLE / "unified-mdk/workspace_compat.py"))
            witness = helpers["product_witness"]
            staged_tests = witness("suite-vault-engine", original, product)
            injected.update(staged_tests["paths"])
            additional = staged_tests["digests"]
        if include_reader:
            reader = witness("suite-reader-stage", original, product / "marmish-prototype")
            additional["prototypeReader"] = reader["bodySha256"]
            appended.add("marmish-prototype/" + reader["leaf"])
        if include_manifest:
            parser_stage = witness("suite-parser", original, product)
            injected.add(parser_stage["vector"])
            appended.add(parser_stage["leaf"])
            additional.update(parser_stage["digests"])
        if include_mdk:
            mdk = witness("suite-mdk-case", original, product / "marmish-mdk")
            appended.add("marmish-mdk/" + mdk["leaf"])
            additional["mdkSyntheticJson"] = mdk["bodySha256"]
        staged = snapshot()
        if set(staged) != set(pristine) | injected or len(appended) != 3 + int(include_reader) + int(include_manifest) + int(include_mdk):
            raise ValueError("unexpected staged file inventory")
        for path, contents in pristine.items():
            if path in appended:
                if staged[path] == contents or not staged[path].startswith(contents):
                    raise ValueError(f"staging replaced generated code in {path}")
            elif staged[path] != contents:
                raise ValueError(f"staging altered {path}")
        witnessed = {}
        registrations = {}
        for harness, target in [(CORE, ["-p","marmish-core","--test","contracts"]),
                                (STREAM, ["-p","marmish-stream","--lib"])]:
            if baseline is not None:
                listing = inside("cargo", "test", "--locked", "--offline", "--release",
                                 *target, "--", "--list", cwd=product).decode()
                registrations[harness] = check_registered_names(harness, listing, baseline["harnesses"][harness]["passed"])
            raw = inside("cargo","test","--locked","--offline","--release", *target, "--","--format","pretty", cwd=product)
            names = assembly["passing_tests"](raw.decode())
            if harness == STREAM:
                names = suite_check("suite-selection", "stream-names", data=names)
            if not names or sorted(names) != sorted(inventory["harnesses"][harness]["tests"]):
                raise ValueError(f"complete harness did not pass: {harness}")
            witnessed[harness] = names
        release = product/"target/release"
        invocations = {}
        mdk_control = {}
        if additional:
            runner = work / "workspace-product"
            shutil.copy2(helpers["workspace_tool"]()[1], runner)
            selected = json.loads(inside(runner, "suite-selected-cases", product, host, suite_kind, cwd=product))
            invocations.update(selected["passingProcesses"])
            witnessed.update(selected["witnessed"])
            mdk_control = selected["mdkZeroTestControl"]
        if include_manifest:
            parsed = json.loads(inside(runner, "suite-parser-cases", product, cwd=product))
            invocations[parsed["harness"]] = parsed["cases"]
            witnessed[parsed["harness"]] = sorted(parsed["cases"])
        # Use Cargo's artifact for the harness target. Native and test builds can
        # legitimately leave multiple feature-specific libc archives in release/deps.
        libc_pin, = [p for p in tomllib.loads((product/"Cargo.lock").read_text())["package"]
                     if p["name"] == "libc"]
        artifacts = inside("cargo","build","--release","--offline","--locked","--lib","--target",host,
                           "-p",f"libc@{libc_pin['version']}","--message-format=json",cwd=product)
        libc, = [Path(path) for line in artifacts.splitlines()
                 if (row:=json.loads(line)).get("reason")=="compiler-artifact" and row["target"]["name"]=="libc"
                 for path in row["filenames"] if path.endswith(".rlib")]
        rustc = Path(run("rustup","which","--toolchain","1.97.1","rustc").decode().strip())
        processes = json.loads(run(
            "cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline",
            "--manifest-path", WHOLE / "canonical-public-data/Cargo.toml",
            "--target-dir", work / "process-test-tool", "--bin", "qualify-process-limits", "--",
            original, "--rustc", rustc, "--libc", libc, "--client", release / "marmish",
            "--daemon", release / "marmishd",
        ))
        if processes["executables"] != binary_hashes:
            raise ValueError("harness received different release binaries")
        witnessed[NATIVE] = [processes["test"]]
        rows, totals = reconcile(inventory, witnessed)
        named_baseline = {} if baseline is None else {
            "canonicalNamedBaselineSha256": check_default_baseline(host, rows),
            "preRunRegisteredNames": registrations,
        }
        return dict(original=PIN, framework=framework_pin, target=host, profile=inventory["selection"],
                    manifestSha256=manifest_digest, workspaceLockSha256=sha(lock),
                    inventorySha256=sha(suite.read_bytes()), upstreamModules=modules,
                    releaseBinarySha256=binary_hashes, nativeProcessTest=processes, harnesses=rows, totals=totals,
                    **({"mdkZeroTestControl": mdk_control, "mdkNormalDependencies": [],
                        "mdkDeniedSourcePaths": list(MDK_DENIED_SOURCES)} if include_mdk else {}),
                    **named_baseline,
                    **({"builtLibraries":built_libraries} if include_reader else {}),
                    **({"selectedTestSources": additional, "passingProcesses": invocations} if additional else {}))


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","compiler_cache","reference_target","receipt"):
        ap.add_argument(name,type=Path)
    ap.add_argument("--require-complete",action="store_true")
    ap.add_argument("--candidate",choices=CANDIDATES,default="v7")
    args = ap.parse_args()
    result = qualify(*(getattr(args,n).resolve() for n in ("original","framework","compiler_cache","reference_target")),candidate=args.candidate)
    args.receipt.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
    print(result["target"], result["totals"])
    if args.require_complete and result["totals"]["unverified"]:
        sys.exit(1)
