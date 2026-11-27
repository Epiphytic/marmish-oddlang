"""Bind selected upstream harnesses to a sealed workspace and count uncovered named tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tomllib
from typing import NamedTuple
import tempfile

from upstream import PIN
import process_limits
import selected_tests
import parser_cases

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
CORE = "marmish-core/test/contracts"
STREAM = "marmish-stream/lib/marmish_stream"
NATIVE = "marmish-native/test/process_limits"
FRAMEWORK = "7ed1c29acde2cc4cfdc58c2489935e703470ae70"
MANIFEST = HERE.parents[2] / "oddlang/portable/workspace-v7-vault-engine.json"
PUBLIC_DATA = "canonical-public-data"
DEFAULT_SUITE_PINS = {
    "aarch64-apple-darwin": "7cfd7edb63815f1e73553786abe5dc0c35535c7b7ea98ea4245db979068c2306",
    "aarch64-unknown-linux-gnu": "4a472c53a1fea65ea7d5b4cf5eb144081aa33e9936e53ff92baa0b3b7359e941",
}

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


def stream_name(compiled):
    prefix, tail = compiled.split("::tests::")
    native = {"copy_caller":"copy_protocol", "wire_caller":"wire"}.get(prefix)
    if native is None:
        match = re.fullmatch(r"(windows|codec)::__projection_\d+", prefix)
        if not match:
            raise ValueError(f"unexpected upstream module mapping: {compiled}")
        native = match[1]
    return f"{native}::tests::{tail}"


def default_baseline(host):
    data = (HERE / f"canonical-{host}.json").read_bytes()
    if sha(data) != DEFAULT_SUITE_PINS[host]:
        raise ValueError("pinned canonical named baseline changed")
    return json.loads(data)


def check_default_baseline(host, harnesses):
    if harnesses != default_baseline(host)["harnesses"]:
        raise ValueError("candidate named baseline differs from the canonical selection")
    return DEFAULT_SUITE_PINS[host]


def check_registered_names(harness, listing, expected):
    normalize = stream_name if harness == STREAM else lambda name: name
    names = [normalize(line.removesuffix(": test")) for line in listing.splitlines()
             if line.endswith(": test")]
    if sorted(names) != sorted(expected):
        raise ValueError(f"unsafe or incomplete test registration for {harness}")
    return sorted(names)


def reconcile(inventory, passed):
    harnesses = inventory["harnesses"]
    if set(passed) - set(harnesses):
        raise ValueError("case receipt names an unknown harness")
    rows = {}
    for target, entry in harnesses.items():
        eligible, ignored = set(entry["tests"]), set(entry["ignored"])
        names = passed.get(target, [])
        if len(eligible) != len(entry["tests"]) or len(ignored) != len(entry["ignored"]) or eligible & ignored:
            raise ValueError(f"invalid test inventory: {target}")
        if len(set(names)) != len(names) or set(names) - eligible:
            raise ValueError(f"duplicate or unrecognized test: {target}")
        rows[target] = dict(passed=sorted(names), unverified=sorted(eligible-set(names)),
                            ignored=sorted(ignored), benchmarks=entry["benchmarks"])
    totals = {key:sum(len(row[key]) for row in rows.values())
              for key in ("passed", "unverified", "ignored", "benchmarks")}
    return rows, totals


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
            for name in selected_tests.MDK_DENIED_SOURCES:
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
            baseline = json.loads(prior_lock.read_bytes())
            if (set(baseline["members"]) != set(seal["members"]) or any(
                    old != seal["members"][name] for name, old in baseline["members"].items()
                    if name != "marmish-mdk")
                    or baseline["workspaceCargoManifestSha256"] != seal["workspaceCargoManifestSha256"]):
                raise ValueError("the MDK test seam changed another member or the workspace root")
            checked = inside("cargo", "tree", "--locked", "--offline", "-p", "marmish-mdk",
                             "-e", "normal", "--prefix", "none", "--format", "{p}", cwd=product).decode()
            if checked.strip().splitlines() != [f"marmish-mdk v0.1.0 ({product / 'marmish-mdk'})"]:
                raise ValueError(f"MDK acquired an ordinary dependency: {checked}")
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
            paths, additional = selected_tests.stage(original, product)
            injected.update(paths)
        if include_reader:
            additional["prototypeReader"] = selected_tests.stage_reader(original,product)
            appended.add(selected_tests.READER_LEAF)
        if include_manifest:
            parser_leaf, parser_prefix, vector_path = parser_cases.stage(original, product)
            injected.add(vector_path)
            appended.add(parser_leaf)
            additional.update(parser_cases.SOURCES)
        if include_mdk:
            leaf, digest = selected_tests.stage_mdk(original, product)
            appended.add(leaf)
            additional["mdkSyntheticJson"] = digest
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
                names = [stream_name(n) for n in names]
            if not names or sorted(names) != sorted(inventory["harnesses"][harness]["tests"]):
                raise ValueError(f"complete harness did not pass: {harness}")
            witnessed[harness] = names
        release = product/"target/release"
        invocations = {}
        mdk_control = {}
        if additional:
            for harness, names in selected_tests.cases(host, include_reader, include_mdk).items():
                package = harness.split("/")[0]
                private_prefixes = {selected_tests.PROTOTYPE: selected_tests.READER_PREFIX,
                                    selected_tests.MDK: selected_tests.MDK_PREFIX}
                target = ["--lib"] if harness in private_prefixes else ["--test", "selected_upstream"]
                test_prefix = private_prefixes.get(harness, "")
                command = ("cargo","test","--locked","--offline","--release","-p",package,
                           *target,"--")
                listing = inside(*command,"--list",cwd=product).decode()
                if sorted(line.removesuffix(": test") for line in listing.splitlines()
                          if line.endswith(": test")) != [test_prefix+n for n in names]:
                    raise ValueError(f"unexpected upstream tests registered for {package}")
                invocations[harness] = {}
                for name in names:
                    registered = test_prefix+name
                    raw = inside(*command,"--exact",registered,"--format","pretty",cwd=product).decode()
                    # Engine starts a second test harness process at this exact name.
                    # Each process must really run the case and report its own success.
                    runs = 2 if harness == selected_tests.ENGINE else 1
                    selected_tests.check_execution(registered, runs, raw, assembly["passing_tests"](raw))
                    invocations[harness][name] = runs
                if harness == selected_tests.MDK:
                    mdk_control = selected_tests.zero_mdk_control(
                        inside, product, command, assembly["passing_tests"])
                witnessed[harness] = names
        if include_manifest:
            invocations[parser_cases.HARNESS] = parser_cases.witness(
                inside, product, parser_prefix, assembly["passing_tests"], selected_tests.check_execution)
            witnessed[parser_cases.HARNESS] = list(parser_cases.NAMES)
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
        processes = process_limits.run(original,rustc,libc,{n:release/n for n in ("marmish","marmishd")},None)
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
                        "mdkDeniedSourcePaths": list(selected_tests.MDK_DENIED_SOURCES)} if include_mdk else {}),
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
