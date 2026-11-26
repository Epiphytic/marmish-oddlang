"""Recover original native record declarations and verify source-free Rust clients."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BUNDLE = "native-journal-records-v1"
THIN = "native-journal-records-package.json"
WORKSPACE = "workspace-v7-vault-engine-records.json"
SOURCE = Path("crates/marmish-native/src")
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
FRAMEWORK = "5c84857fe308c61be1eaa597137d31eb53c34146"
EXPECTED = [
    r'{"session":"s\n1","group_id":"group","peer":"北","revoked":true}',
    r'{"application_id":"app","app_event_id":"event","state":"sent"}',
    r'{"cursor":18446744073709551615,"session":"s\n1","application_id":"app","sender":"北","data":[0,127,255],"invalidated":false}',
]
ALTERNATE = [*EXPECTED[:2], EXPECTED[2].replace(
    r'"cursor":18446744073709551615,"session":"s\n1"',
    r'"session":"s\n1","cursor":18446744073709551615', 1)]


def run(*argv, cwd=None, env=None):
    result = subprocess.run(argv, cwd=cwd, env=env, check=False, capture_output=True, timeout=600)
    if result.returncode:
        raise RuntimeError(f"command failed: {argv}\n{result.stderr.decode(errors='replace')}")
    return result.stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def json_file(path):
    return json.loads(path.read_bytes())


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def pin(base, path):
    return {"path": path.relative_to(base).as_posix(), "sha256": sha(path.read_bytes())}


def cargo_env():
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.97.1", CARGO_NET_OFFLINE="true",
               MARMISH_BUILD_COMMIT=ORIGINAL)
    for key in ("CARGO_BUILD_TARGET", "CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(key, None)
    return env


def probe(output, messages):
    libraries = {}
    native_paths = set()
    for line in messages.splitlines():
        item = json.loads(line)
        if item["reason"] == "compiler-artifact":
            name = item["target"]["name"]
            if name in ("marmish_native", "serde_json"):
                paths = [Path(path) for path in item["filenames"] if path.endswith(".rlib")]
                if len(paths) == 1:
                    if name in libraries:
                        raise ValueError(f"ambiguous {name} library in Cargo records")
                    libraries[name] = paths[0]
        elif item["reason"] == "build-script-executed":
            native_paths.update(item["linked_paths"])
    if set(libraries) != {"marmish_native", "serde_json"}:
        raise ValueError(f"unexpected native caller libraries: {libraries}")
    flags = ["-L", f"dependency={libraries['serde_json'].parent}"]
    for name, path in sorted(libraries.items()):
        flags.extend(("--extern", f"{name}={path}"))
    for path in sorted(native_paths):
        flags.extend(("-L", path))
    run("rustc", "+1.97.1", "--edition=2024", HERE / "probe.rs", *flags, "-o", output)
    return {"jsonLines": run(output, cwd=output.parent).decode().splitlines(),
            "libraries": {name: sha(path.read_bytes()) for name, path in libraries.items()}}


def mutate(original, mutant):
    # The reordered public fields keep the original implementation and call sites valid.
    archive = mutant.parent / "original.tar"
    with archive.open("wb") as output:
        subprocess.run(["git", "-C", original, "archive", ORIGINAL], stdout=output, check=True)
    mutant.mkdir()
    run("tar", "-xf", archive, "-C", mutant)
    file = mutant / SOURCE / "journal.rs"
    prefix = b"pub struct ReceivedMessage {\n"
    old = prefix + b"    pub cursor: u64,\n    pub session: String,\n"
    replacement = prefix + b"    pub session: String,\n    pub cursor: u64,\n"
    original_source = file.read_bytes()
    if original_source.count(old) != 1:
        raise ValueError("source mutation no longer changes exactly the two admitted fields")
    file.write_bytes(original_source.replace(old, replacement))


def recover(compiler, original, mutant, scratch):
    selection = PORTABLE / BUNDLE / "selection.json"
    baseline, changed = scratch/"recovered", scratch/"recovered-reordered"
    run(compiler, "recover", "public-records", original/SOURCE/"lib.rs", selection, baseline)
    for name in ("graph.json", "rust.json", "recovery.lock"):
        if (baseline/name).read_bytes() != (PORTABLE/BUNDLE/name).read_bytes():
            raise ValueError(f"committed record recovery differs: {name}")
    altered = json_file(selection)
    part, = [part for part in altered["modules"] if part["name"] == "journal"]
    part["sha256"] = sha((mutant/SOURCE/"journal.rs").read_bytes())
    changed_selection = scratch/"mutated-selection.json"
    write_json(changed_selection, altered)
    run(compiler, "recover", "public-records", mutant/SOURCE/"lib.rs", changed_selection, changed)
    graph = json_file(baseline/"graph.json")
    record, = [record for module in graph["modules"] for record in module["records"]
               if record["id"] == "urn:marmish:native:journal:ReceivedMessage"]
    record["fields"][:2] = record["fields"][:2][::-1]
    if graph != json_file(changed/"graph.json") or (baseline/"rust.json").read_bytes() != (changed/"rust.json").read_bytes():
        raise ValueError("mutation changed the graph beyond the declared field permutation")
    return changed


def stage_thin(dst, graphs):
    dst.mkdir()
    bundle = dst / BUNDLE
    bundle.mkdir()
    manifest = json_file(PORTABLE / THIN)
    for name in ("graph.json", "rust.json"):
        shutil.copyfile(graphs / name, bundle / name)
    shutil.copyfile(PORTABLE / BUNDLE / "Cargo.lock", bundle / "Cargo.lock")
    component, = manifest["components"]
    component["graph"] = pin(dst, bundle/"graph.json")
    component["overlay"] = pin(dst, bundle/"rust.json")
    out = dst/THIN
    write_json(out, manifest)
    return out


def sandbox(assembly, work, paths, provider_cache):
    if sys.platform == "linux":
        return assembly["linux_isolation"](work, provider_cache)
    tools = assembly["envtools"]
    readable, env = tools.isolation_environment(work, "aarch64-apple-darwin")
    sdk = ("xcrun", "--sdk", "macosx", "--find")
    make = Path(tools.run(*sdk, "make").strip())
    env.update(CC=env["CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER"], AR=tools.run(*sdk, "ar").strip(),
               RANLIB=tools.run(*sdk, "ranlib").strip(), MACOSX_DEPLOYMENT_TARGET="11.0",
               XCRUN_CACHE_DISABLED="1", PATH=f"{make.parent}:{env['PATH']}")
    xcode = next(parent for parent in Path(env["DEVELOPER_DIR"]).parents if parent.suffix == ".app")
    if provider_cache:
        (Path(env["CARGO_HOME"])/"git").symlink_to(provider_cache, target_is_directory=True)
    return tools.isolated(work, (*readable, xcode), *paths), env


def qualify(work, scratch, args, assembly):
    compiler_target = work/"compiler"
    run("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", compiler_target, cwd=args.framework, env=cargo_env())
    compiler = compiler_target/"debug/oddlang"
    mutant = scratch/"original-mutant"
    mutate(args.original, mutant)
    changed = recover(compiler, args.original, mutant, scratch)
    reference_build = ("cargo", "+1.97.1", "build", "--locked", "--offline", "--release",
        "-p", "marmish-native", "--lib", "--message-format=json", "--target-dir")
    original_output = probe(scratch/"native-original",
        run(*reference_build, args.original_target, cwd=args.original, env=cargo_env()))
    changed_original = probe(scratch/"native-reordered",
        run(*reference_build, scratch/"mutant-target", cwd=mutant, env=cargo_env()))

    ready = work/"inputs"
    if args.standalone:
        base_manifest = stage_thin(ready, PORTABLE/BUNDLE)
        provider_cache = None
    else:
        ready.mkdir()
        base_manifest = ready/WORKSPACE
        assembly["stage_workspace"](PORTABLE/WORKSPACE, ready)
        provider_cache = work/"providers"
        root_lock = PORTABLE/json_file(PORTABLE/WORKSPACE)["cargoLock"]["path"]
        assembly["stage_provider_git_cache"](root_lock, provider_cache)
    changed_manifest = stage_thin(work/"mutant-inputs", changed)
    assert all(path.suffix in {".json", ".ttl", ".lock"}
               for inputs in (ready, changed_manifest.parent)
               for path in inputs.rglob("*") if path.is_file())
    prefix, env = sandbox(assembly, work,
        (ROOT, args.original, args.framework, args.original_target, scratch), provider_cache)

    def invocation(*argv, cwd=work):
        command = list(prefix)
        if sys.platform == "linux":
            command[command.index("--chdir")+1] = str(cwd)
        return [*command, *argv]

    probes = [args.original/SOURCE/"journal.rs", mutant/SOURCE/"journal.rs", HERE/"probe.rs",
              args.framework/"src/lib.rs", PORTABLE/BUNDLE/"selection.json"]
    for excluded in probes:
        check = subprocess.run(invocation("/bin/cat", excluded), cwd=work, env=env, capture_output=True)
        if not check.returncode or not any(error in check.stderr for error in
                                           (b"Operation not permitted", b"No such file or directory")):
            raise ValueError(f"projector could read excluded input {excluded}")
    generated = {}
    profiles = [
        ("baseline", base_manifest, args.standalone,
         PORTABLE/BUNDLE/"package.lock" if args.standalone else
         PORTABLE/Path(WORKSPACE).stem/"workspace.lock"),
        ("reordered", changed_manifest, True, None),
    ]
    for name, manifest, thin, expected_lock in profiles:
        product = work/f"generated-{name}"
        kind = "native-package" if thin else "native-workspace"
        run(*invocation(compiler, "build", kind, "rust", manifest, product), cwd=work, env=env)
        lock = product/("package.lock" if thin else "workspace.lock")
        lock_bytes = lock.read_bytes()
        if expected_lock is not None and lock_bytes != expected_lock.read_bytes():
            raise ValueError(f"{name} sealed {kind} lock mismatch")
        if not thin:
            inherited = json_file(PORTABLE/"workspace-v7-vault-engine/workspace.lock")
            current = json.loads(lock_bytes)
            if current["cargoLockSha256"] != inherited["cargoLockSha256"]:
                raise ValueError("the journal package changed the combined Cargo pin")
            for member, previous in inherited["members"].items():
                observed = current["members"][member]
                if member == "marmish-native":
                    stable = ("workspaceCargoManifestSha256", "dependencies")
                    if any(observed[key] != previous[key] for key in stable):
                        raise ValueError("the journal package changed existing native Cargo edges")
                elif observed != previous:
                    raise ValueError(f"journal records changed another member: {member}")
        # In the combined profile Cargo compiles all libraries and both normal binary entrypoints.
        selection = ["--lib"] if thin else ["--workspace"]
        records = run(*invocation("cargo", "build", "--release", "--offline", "--locked",
            "--message-format=json", *selection, cwd=product), cwd=product, env=env)
        generated[name] = probe(scratch/f"candidate-{name}", records) | {
            "inputSha256": sha(manifest.read_bytes()), "lockSha256": sha(lock_bytes),
        }

    for name, oracle, expected in (("baseline",original_output,EXPECTED),
                                   ("reordered",changed_original,ALTERNATE)):
        if oracle["jsonLines"] != expected or generated[name]["jsonLines"] != expected:
            raise ValueError(f"native public records differ in profile {name}")
    return {
        "format":"marmish-journal-record-evidence-v1",
        "original":ORIGINAL, "framework":FRAMEWORK,
        "companion":run("git","rev-parse","HEAD",cwd=ROOT).decode().strip(),
        "rustc":run("rustc","+1.97.1","-vV").decode().splitlines(),
        "sealedBaseline":"standalone" if args.standalone else "combined-v7-vault-engine",
        "sourceDenialProbes":["original-module","mutated-module","verification-caller","compiler-source","recovery-selection"],
        "publicRecords":3, "explicitReexports":3, "orderedFields":13,
        "nativeCallerSha256":sha((HERE/"probe.rs").read_bytes()),
        "baselineRecovery":json_file(PORTABLE/BUNDLE/"recovery.lock"),
        "reorderedRecovery":json_file(changed/"recovery.lock"),
        "originalBaseline":original_output, "originalReordered":changed_original,
        "generated":generated,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--original-target", type=Path, required=True)
    parser.add_argument("--standalone", action="store_true")
    args = parser.parse_args()
    args.framework = args.framework.resolve()
    args.original = args.original.resolve()
    args.original_target = args.original_target.resolve()
    for path, expected in ((args.framework, FRAMEWORK), (args.original, ORIGINAL)):
        if (run("git", "rev-parse", "HEAD",cwd=path).decode().strip() != expected
            or run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)):
            raise ValueError(f"expected clean pinned checkout: {path}")
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT):
        raise ValueError("expected clean companion qualification checkout")
    assembly = runpy.run_path(str(HERE.parent/"workspace/verify.py"))
    with (tempfile.TemporaryDirectory() as temp, tempfile.TemporaryDirectory() as outside):
        result = qualify(Path(temp).resolve(), Path(outside).resolve(), args, assembly)
    write_json(args.receipt, result)


if __name__ == "__main__":
    main()
