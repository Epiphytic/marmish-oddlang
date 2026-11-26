#!/usr/bin/env python3
"""Qualify a source-isolated core projection against a pinned Rust workspace."""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
RUST_COMMIT = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
RUST = "1.97.1"
FRAMEWORK = "a635eec8e0f71d2dabc9aee28655e26494a7506f"
HERE = Path(__file__).resolve().parent
COMPANION = HERE.parents[2]
PORTABLE = COMPANION / "oddlang/portable"
MANIFEST = PORTABLE / "core-package.json"
LOCK = PORTABLE / "core-package-v1/package.lock"
TEST = Path("crates/marmish-core/tests/contracts.rs")
CALLER = HERE / "caller.rs"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run(*args, cwd=None, env=None, fail=None):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True)
    if fail is not None:
        require(result.returncode != 0 and fail in result.stderr,
                f"negative control did not report {fail!r}: {args}\n{result.stdout}{result.stderr}")
    elif result.returncode:
        raise ValueError(f"exit {result.returncode}: {args}\n{result.stdout}{result.stderr}")
    return result.stdout


def isolation_environment(work, triple):
    toolchain = Path(run("rustc", f"+{RUST}", "--print", "sysroot").strip()).resolve()
    developer = Path(run("xcode-select", "-p").strip()).resolve()
    sdk = Path(run("xcrun", "--sdk", "macosx", "--show-sdk-path").strip()).resolve()
    linker = Path(run("xcrun", "--sdk", "macosx", "--find", "clang").strip()).resolve()
    default_cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    registry = (default_cargo_home / "registry").resolve()
    cargo_home = work / "candidate-cargo"
    cargo_home.mkdir()
    (cargo_home / "registry").symlink_to(registry, target_is_directory=True)
    temp = work / "tmp"
    temp.mkdir()
    env = {
        "PATH": f"{toolchain / 'bin'}:/usr/bin:/bin",
        "HOME": str(work),
        "CARGO_HOME": str(cargo_home),
        "RUSTC": str(toolchain / "bin/rustc"),
        "RUSTDOC": str(toolchain / "bin/rustdoc"),
        "TMPDIR": str(temp),
        "CARGO_NET_OFFLINE": "true",
        "LC_ALL": "C",
        "DEVELOPER_DIR": str(developer),
        "SDKROOT": str(sdk),
        f"CARGO_TARGET_{triple.upper().replace('-', '_')}_LINKER": str(linker),
    }
    return (toolchain, developer, registry, sdk, linker.parent), env


def isolated(work, read_roots, *denied_paths):
    sandbox = Path("/usr/bin/sandbox-exec")
    require(sys.platform == "darwin" and sandbox.is_file(),
            "this release receipt requires a macOS Seatbelt host")
    system = [Path(path) for path in ("/System", "/usr", "/bin", "/sbin", "/Library/Developer",
                                      "/private/etc/ssl")]
    inputs = system + list(read_roots) + [work]
    reads = " ".join(f"(subpath {json.dumps(str(path))})" for path in inputs)
    denied = " ".join(f"(subpath {json.dumps(str(path))})" for path in denied_paths)
    workspace = json.dumps(str(work))
    policy = f"""
(version 1)
(deny default)
(allow process* sysctl-read mach-lookup file-read-metadata)
(allow file-read* {reads}
  (literal "/") (literal "/dev/null") (literal "/dev/random") (literal "/dev/urandom")
  (literal "/private/etc/localtime"))
(allow file-write* (subpath {workspace}) (literal "/dev/null"))
(deny file-read* {denied})
(deny network*)
"""
    return (sandbox, "-p", policy)


def build_framework(framework, work):
    require(run("git", "-C", framework, "rev-parse", "HEAD").strip() == FRAMEWORK,
            "unexpected framework revision")
    require(not run("git", "-C", framework, "status", "--porcelain", "--untracked-files=all"),
            "framework checkout has local changes")
    target = work / "framework-target"
    run("cargo", f"+{RUST}", "build", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", target, cwd=framework, env=cargo_environment())
    return target / "debug/oddlang"


def stage_inputs(root):
    root.mkdir()
    manifest = read(MANIFEST)
    inputs = [manifest["cargoLock"]]
    for component in manifest["components"]:
        inputs.extend((component["graph"], component["overlay"]))
    for item in inputs:
        relative = Path(item["path"])
        require(not relative.is_absolute() and ".." not in relative.parts, f"unsafe input {relative}")
        source = (PORTABLE / relative).resolve(strict=True)
        require(source.is_relative_to(PORTABLE), f"input escapes portable directory: {relative}")
        require(sha(source) == item["sha256"], f"input digest mismatch: {item['path']}")
        out = root / relative
        out.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, out)
    shutil.copyfile(MANIFEST, root / MANIFEST.name)


def inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        require(not path.is_symlink(), f"unexpected symbolic link in inventory: {path.name}")
        if path.is_file():
            result[path.relative_to(root).as_posix()] = sha(path)
    return result


def verify_lock(package, manifest_path):
    manifest = read(manifest_path)
    lock_path = package / "package.lock"
    lock = read(lock_path)
    require(lock["format"] == "oddlang-native-package-lock-v1", "package lock format")
    generated = {f"src/{name}": digest for name, digest in inventory(package / "src").items()}
    require(lock["generatedFiles"] == generated, "generated source inventory differs from lock")
    require(lock["manifestSha256"] == sha(manifest_path), "manifest lock mismatch")
    require(lock["cargoManifestSha256"] == sha(package / "Cargo.toml"), "Cargo.toml lock mismatch")
    cargo_lock = package / "Cargo.lock"
    require(lock["cargoLockSha256"] == manifest["cargoLock"]["sha256"] == sha(cargo_lock),
            "Cargo.lock mismatch")
    for dependency in tomllib.loads(cargo_lock.read_text())["package"]:
        if dependency["name"] != manifest["name"]:
            require(dependency.get("source") == "registry+https://github.com/rust-lang/crates.io-index"
                    and re.fullmatch(r"[0-9a-f]{64}", dependency.get("checksum", "")),
                    f"dependency is not pinned in the registry: {dependency['name']}")
    require(len(lock["components"]) == len(manifest["components"]), "component lock count")
    groups = {}
    for ordinal, (component, pinned) in enumerate(zip(manifest["components"], lock["components"])):
        for name in ("kind", "module", "reexports"):
            require(component[name] == pinned[name], f"component {ordinal} {name} mismatch")
        for name in ("graph", "overlay"):
            require(component[name]["sha256"] == pinned[f"{name}Sha256"],
                    f"component {ordinal} {name} digest mismatch")
        group = groups.setdefault(pinned["module"], ordinal)
        source = package / "src" / f"group_{group}" / f"__projection_{ordinal}.rs"
        require(sha(source) == pinned["sourceSha256"], f"component {ordinal} source digest mismatch")
    return lock_path


def cargo_environment():
    excluded = {"RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_BOOTSTRAP",
                "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET"}
    env = {key: value for key, value in os.environ.items()
           if key not in excluded and not key.startswith("CARGO_PROFILE_")}
    env.update(RUSTUP_TOOLCHAIN=RUST, CARGO_NET_OFFLINE="true")
    return env


def cargo(prefix, workspace, target, action, *, test=None, transcript=None, candidate_env=None):
    env = cargo_environment() if candidate_env is None else candidate_env.copy()
    if transcript is not None:
        env["ODDLANG_CORE_TRANSCRIPT"] = str(transcript)
    command = ["cargo", f"+{RUST}"] if candidate_env is None else ["cargo"]
    args = [*prefix, *command, action, "--release", "--locked", "--offline",
            "--target-dir", target, "--package", "marmish-core"]
    if test is not None:
        args.extend(("--test", test))
    return run(*args, cwd=workspace, env=env)


def cases(path):
    result = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        family, name, observation = json.loads(line)
        key = (family, name)
        require(key not in result, f"duplicate observation {key}")
        result[key] = observation
    return result


def expectations(reference):
    expected = {
        ("relay", "empty"): {"error": "EmptyRelaySet"},
        ("relay", "raw-at-before-scheme"): {"error": "DisallowedComponents"},
        ("relay", "raw-whitespace-before-at"): {"error": "InvalidUrl"},
        ("relay", "loopback-label"): {"error": "LoopbackDenied"},
        ("relay", "ipv4-private-with-opt-in-opt-in"): {"error": "DisallowedHost"},
        ("relay", "ipv6-deny-within-allow-opt-in"): {"error": "DisallowedHost"},
        ("relay", "stable-dedup"): {"urls": ["wss://first.example/", "wss://xn--bcher-kva.example/",
                                            "ws://third.example/"]},
        ("decode", "tuple-good"): {"bytes": [0, 1, 255]},
        ("decode", "tuple-escaped"): {"bytes": [102]},
        ("decode", "tuple-long"): {"error": "InvalidJson"},
        ("decode", "json-before-version"): {"error": "InvalidJson"},
        ("decode", "tuple-version-before-encoding"): {"error": "InvalidVersion"},
        ("decode", "payload-nonzero-tail"): {"error": "InvalidEncoding"},
        ("decode", "wire-32768"): {"bytes": []},
        ("decode", "wire-32769"): {"error": "SizeLimit"},
        ("decode", "encoded-1-21848"): {"error": "SizeLimit"},
        ("decode", "malformed-encoded-1-21848"): {"error": "InvalidEncoding"},
        ("decode", "malformed-encoded-2-21849"): {"error": "InvalidVersion"},
        ("encode", "raw-16385"): {"error": "SizeLimit"},
        ("retention", "Some(31536000)"): {"seconds": 31536000},
    }
    for ordinal, name in enumerate(("SizeLimit", "InvalidJson", "InvalidVersion", "InvalidEncoding")):
        expected[("frame-error", name)] = {"ordinal": ordinal, "error": name, "hasSource": False}
    for key, values in expected.items():
        observed = reference[key]
        require(all(observed[name] == value for name, value in values.items()),
                f"original outcome differs from independent expectation: {key}")


def change_once(path, old, new):
    data = path.read_bytes()
    require(data.count(old) == 1, f"mutation should change exactly one occurrence in {path.name}")
    path.write_bytes(data.replace(old, new, 1))


def mutate(root, name):
    path = root / MANIFEST.name
    manifest = read(path)
    if name == "retention-bound":
        pinned = manifest["components"][0]["graph"]
        change_once(root / pinned["path"], b"c:maximum 31536000", b"c:maximum 31535999")
    elif name == "relay-rule-order":
        pinned = manifest["components"][1]["graph"]
        graph_path = root / pinned["path"]
        graph = read(graph_path)
        graph["rules"][0], graph["rules"][1] = graph["rules"][1], graph["rules"][0]
        write(graph_path, graph)
    elif name == "frame-version":
        pinned = manifest["components"][2]["graph"]
        change_once(root / pinned["path"], b"c:bits 32 ; c:value 1", b"c:bits 32 ; c:value 2")
    elif name == "frame-error-order":
        pinned = manifest["components"][2]["overlay"]
        overlay_path = root / pinned["path"]
        overlay = read(overlay_path)
        overlay["variantOrder"][0], overlay["variantOrder"][1] = (
            overlay["variantOrder"][1], overlay["variantOrder"][0])
        write(overlay_path, overlay)
    else:
        raise ValueError(f"unknown mutation {name}")
    pinned["sha256"] = sha(root / pinned["path"])
    write(path, manifest)


def install_caller(root):
    tests = root / "tests"
    tests.mkdir(exist_ok=True)
    shutil.copyfile(CALLER, tests / "differential.rs")


def test_results(output):
    return [tuple(map(int, group)) for group in
            re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", output)]


def qualify(original, framework, work):
    compiler = run("rustc", f"+{RUST}", "-vV")
    require(f"commit-hash: {RUST_COMMIT}" in compiler.splitlines(), "unexpected compiler commit")
    triple = next(row.removeprefix("host: ") for row in compiler.splitlines() if row.startswith("host: "))
    require(run("git", "-C", original, "rev-parse", "HEAD").strip() == ORIGINAL,
            "unexpected original revision")
    require(not run("git", "-C", original, "status", "--porcelain", "--untracked-files=all"),
            "original checkout has local changes")

    oddlang = build_framework(framework, work)
    read_roots, candidate_env = isolation_environment(work, triple)
    sandbox = isolated(work, read_roots, original, COMPANION, framework)
    for canary in (original / "Cargo.toml", COMPANION / "Cargo.toml", framework / "Cargo.toml"):
        require(canary.is_file(), f"unavailable source-isolation canary {canary}")
        run(*sandbox, "/bin/cat", canary, cwd=work, env=candidate_env,
            fail="Operation not permitted")
    with tempfile.TemporaryDirectory(prefix="oddlang-unmounted-core-") as other:
        canary = Path(other).resolve() / "config.rs"
        shutil.copyfile(original / "crates/marmish-core/src/config.rs", canary)
        run(*sandbox, "/bin/cat", canary, cwd=work, env=candidate_env,
            fail="Operation not permitted")
    run(*sandbox, "/bin/cat", "/dev/null", cwd=work, env=candidate_env)
    canary_source = work / "network-canary.rs"
    canary_source.write_text("""
fn main() {
    let port = std::env::args().nth(1).unwrap().parse::<u16>().unwrap();
    if let Err(error) = std::net::TcpStream::connect(("127.0.0.1", port)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
""")
    canary_bin = work / "network-canary"
    run("rustc", f"+{RUST}", canary_source, "-o", canary_bin, cwd=work)
    canary_source.unlink()
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        port = listener.getsockname()[1]
        run(*sandbox, canary_bin, str(port), cwd=work, env=candidate_env,
            fail="Operation not permitted")

    def generate(inputs, package):
        manifest = inputs / MANIFEST.name
        run(*sandbox, oddlang, "build", "native-package", "rust", manifest, package, cwd=work, env=candidate_env)
        return verify_lock(package, manifest)

    inputs = work / "inputs"
    stage_inputs(inputs)
    projected = work / "projected"
    lock = generate(inputs, projected)
    require(lock.read_bytes() == LOCK.read_bytes(), "generated package.lock differs from release")
    duplicate = work / "duplicate"
    generate(inputs, duplicate)
    outputs = inventory(projected)
    require(outputs == inventory(duplicate), "repeated source-free generation differs")
    target = work / "generated-target"
    cargo(sandbox, projected, target, "build", candidate_env=candidate_env)

    bad = work / "wrong-digest"
    stage_inputs(bad)
    change_once(bad / "core-frame-v1/domain.ttl", b"c:bits 32 ; c:value 1", b"c:bits 32 ; c:value 2")
    run(*sandbox, oddlang, "build", "native-package", "rust", bad / MANIFEST.name,
        work / "wrong-digest-output", cwd=work, env=candidate_env, fail="digest mismatch")
    require(not (work / "wrong-digest-output").exists(), "invalid package was published")

    # The original source and unchanged tests enter the scratch tree after the
    # generated release is built. The candidate sandbox also denies the copy.
    reference = work / "reference"
    reference.mkdir()
    archive = work / "original.tar"
    with archive.open("wb") as stream:
        subprocess.run(["git", "-C", original, "archive", ORIGINAL], stdout=stream, check=True)
    with tarfile.open(archive) as tar:
        tar.extractall(reference, filter="data")
    archive.unlink()
    test = (reference / TEST).read_bytes()
    original_target = work / "reference-target"
    original_rows, emitted_rows = work / "original.jsonl", work / "emitted.jsonl"
    reference_sandbox = isolated(work, read_roots, original, COMPANION, framework,
                                 reference, original_target, original_rows)
    run(*reference_sandbox, "/bin/cat", reference / TEST, cwd=work, env=candidate_env,
        fail="Operation not permitted")
    (projected / "tests").mkdir()
    (projected / "tests/contracts.rs").write_bytes(test)
    require(sha(reference / TEST) == sha(projected / "tests/contracts.rs"), "changed upstream tests")

    original_results = test_results(cargo((), reference, original_target, "test"))
    emitted_results = test_results(cargo(reference_sandbox, projected, target, "test",
                                        candidate_env=candidate_env))
    require(original_results == emitted_results and sum(row[0] for row in original_results) == 7
            and all(failed == ignored == 0 for _, failed, ignored in original_results),
            f"unchanged upstream tests differ: {original_results} versus {emitted_results}")
    install_caller(reference / "crates/marmish-core")
    install_caller(projected)
    cargo((), reference, original_target, "test", test="differential", transcript=original_rows)
    run(*reference_sandbox, "/bin/cat", original_rows, cwd=work, env=candidate_env,
        fail="Operation not permitted")
    run(*reference_sandbox, "/bin/ls", original_target, cwd=work, env=candidate_env,
        fail="Operation not permitted")
    cargo(reference_sandbox, projected, target, "test", test="differential",
          transcript=emitted_rows, candidate_env=candidate_env)
    expected, actual = cases(original_rows), cases(emitted_rows)
    expectations(expected)
    require(expected.keys() == actual.keys(), "different differential observation coverage")
    for key, row in expected.items():
        if actual[key] != row:
            raise ValueError(f"native differential mismatch at {key!r}: {str(row)[:300]} "
                             f"!= {str(actual[key])[:300]}")
    require(sha(original_rows) == sha(emitted_rows), "native transcripts differ in bytes")

    witnesses = {
        "retention-bound": (("retention", "Some(31536000)"), "error", "RetentionOutOfRange"),
        "relay-rule-order": (("relay", "raw-whitespace-before-at"), "error", "DisallowedComponents"),
        "frame-version": (("decode", "tuple-good"), "error", "InvalidVersion"),
        "frame-error-order": (("frame-error", "SizeLimit"), "ordinal", 1),
    }
    mutants = {}
    for name, (case, property_name, expected_value) in witnesses.items():
        mutated = work / f"{name}-inputs"
        stage_inputs(mutated)
        mutate(mutated, name)
        out = work / f"{name}-crate"
        mutant_lock = generate(mutated, out)
        install_caller(out)
        witness = work / f"{name}.jsonl"
        cargo(reference_sandbox, out, target, "test", test="differential", transcript=witness,
              candidate_env=candidate_env)
        changed = cases(witness)
        require(changed[case][property_name] == expected_value and changed[case] != expected[case],
                f"mutant survived at {case}")
        mutants[name] = {"case": list(case), "original": expected[case], "mutated": changed[case],
                         "packageLockSha256": sha(mutant_lock), "transcriptSha256": sha(witness)}

    return {
        "originalRevision": ORIGINAL, "frameworkRevision": FRAMEWORK, "compilerCommit": RUST_COMMIT,
        "host": triple, "originalCargoLockSha256": sha(original / "Cargo.lock"),
        "sourceIsolation": {"provider": "macOS Seatbelt allowlist", "deniedReadCanaries": 7,
                            "networkDenied": True, "deniedConnectCanary": True,
                            "referenceCopiedAfterReleaseBuild": True},
        "originalPublicSource": inventory(reference / "crates/marmish-core/src"),
        "unchangedTestSha256": sha(reference / TEST), "unchangedTestsPassedPerImplementation": 7,
        "unchangedTestGroupCounts": original_results,
        "callerSha256": sha(CALLER), "packageManifestSha256": sha(MANIFEST),
        "generatedRelease": outputs, "twoGenerationsEqual": True,
        "differentialCounts": dict(sorted(Counter(family for family, _ in expected).items())),
        "differentialTranscriptSha256": sha(original_rows),
        "mutations": mutants, "rejectedGraphDigestDrift": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("framework", type=Path)
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="oddlang-core-package-") as temporary:
        receipt = qualify(args.original.resolve(), args.framework.resolve(), Path(temporary).resolve())
    write(args.receipt, receipt)
    print(f"Qualified {sum(receipt['differentialCounts'].values())} matching native observations")


if __name__ == "__main__":
    main()
