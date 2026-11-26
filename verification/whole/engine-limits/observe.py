"""Compare the pinned engine and its source-free projection in isolated children."""
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
ROLES = ("marmish", "marmishd")


def run(*argv, cwd, env=None, check=True, preexec_fn=None):
    result = subprocess.run(argv, cwd=cwd, env=env, check=False, capture_output=True,
                            timeout=360, preexec_fn=preexec_fn)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def reference_caller(root, reference, target):
    """Resolve exact rlibs and native search paths from Cargo's own build records."""
    result = run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline",
                 "--package", "marmish-engine", "--lib", "--target-dir", target,
                 "--message-format", "json", cwd=reference)
    records = [json.loads(line) for line in result.stdout.splitlines()]
    libs = {}
    native = set()
    for record in records:
        if record["reason"] == "compiler-artifact":
            name = record["target"]["name"]
            if name in ("marmish_engine", "libc"):
                found = [name for name in record["filenames"] if name.endswith(".rlib")]
                if len(found) == 1:
                    libs[name] = Path(found[0])
        if record["reason"] == "build-script-executed":
            native.update(record["linked_paths"])
    return compile_caller(root, "original", libs, native)


def generated_caller(root, workspace):
    deps = workspace / "target/release/deps"
    libs = {}
    for name in ("marmish_engine", "libc"):
        libs[name], = deps.glob(f"lib{name}-*.rlib")
    return compile_caller(root, "generated", libs, ())


def compile_caller(root, role, libs, native):
    if libs.keys() != {"marmish_engine", "libc"}:
        raise ValueError(f"incomplete {role} library set")
    exe = root / f"observe-{role}"
    flags = ["-L", f"dependency={libs['libc'].parent}"]
    for label, path in sorted(libs.items()):
        flags.extend(("--extern", f"{label}={path}"))
    for path in sorted(native):
        flags.extend(("-L", path))
    run("rustc", "+1.97.1", "--edition", "2021", HERE/"observe.rs", *flags, "-o", exe, cwd=root)
    return exe, {label: sha(path) for label, path in libs.items()}


def file_limits(root, caller):
    hard = resource.getrlimit(resource.RLIMIT_NOFILE)[1]
    rows = []
    for soft in (1024, 65536, 65537):
        def set_soft():
            resource.setrlimit(resource.RLIMIT_NOFILE, (soft, hard))
        got = run(caller, cwd=root, preexec_fn=set_soft)
        observed = tuple(map(int, got.stdout.split()))
        expected = min(soft, 65536) if sys.platform == "darwin" else soft
        if observed[:2] != (soft, expected) or got.stderr:
            raise ValueError((observed, soft, expected, got.stderr))
        rows.append({"beforeSoft": soft, "afterSoft": expected, "hard": observed[2],
                     "twoCallsIdempotent": True, "bothDescriptorFlagsPreserved": True})
    return rows


def startup(root, original, generated):
    darwin = sys.platform == "darwin"
    ordinary = [("--help",), ("--version",), ("--an-unknown-argument",), ("release-info",)]
    if darwin:
        image = root/"deny-rlimit.dylib"
        run("xcrun", "--sdk", "macosx", "clang", "-Wall", "-Werror", "-dynamiclib",
            HERE/"deny_darwin.c", "-o", image, cwd=root)
    records = []
    hard = resource.getrlimit(resource.RLIMIT_NOFILE)[1]

    def set_high():
        resource.setrlimit(resource.RLIMIT_NOFILE, (65537, hard))

    for name in ROLES:
        a, b = (directory / name for directory in (original, generated))
        for args in ordinary:
            left, right = [run(x, *args, cwd=root, check=False, preexec_fn=set_high) for x in (a,b)]
            want = (left.returncode, left.stdout, left.stderr)
            if (right.returncode, right.stdout, right.stderr) != want:
                raise ValueError(f"{name} {args}: startup changed parser behavior")
            records.append(dict(binary=name, arguments=args, exit=want[0],
                stdout=want[1].decode(), stderr=want[2].decode(), fault=None))
        if darwin:
            # The ordinary invalid-argument outcome differs from the startup failure.
            env = dict(os.environ, DYLD_INSERT_LIBRARIES=str(image))
            for x in (a,b):
                result = run(x, *ordinary[2], cwd=root, env=env, check=False, preexec_fn=set_high)
                if (result.returncode, result.stdout, result.stderr) != (1,b"",b"marmish: process_limits\n"):
                    raise ValueError(f"{x}: process resource fault did not preempt CLI: {result}")
            records.append(dict(binary=name, arguments=ordinary[2], exit=1, stdout="",
                stderr="marmish: process_limits\n", fault="getrlimit NOFILE returns EPERM"))
    return {"originalBinaries": {name: sha(original/name) for name in ROLES},
            "generatedBinaries": {name: sha(generated/name) for name in ROLES}, "argvCases": records}


def observe(root, original, original_binaries, generated, original_cargo_target):
    if run("git","rev-parse","HEAD",cwd=original).stdout.decode().strip() != ORIGINAL:
        raise ValueError("unexpected original revision")
    a, a_hashes = reference_caller(root, original, original_cargo_target)
    b, b_hashes = generated_caller(root, generated)
    reference = file_limits(root,a)
    if file_limits(root,b) != reference:
        raise ValueError("process resource observation differs")
    return {"libraryCases": reference, "originalLibraries": a_hashes, "generatedLibraries": b_hashes,
            "observerSha256": sha(HERE/"observe.rs"), "darwinFaultObserverSha256": sha(HERE/"deny_darwin.c"),
            **startup(root, original_binaries, generated/"target/release")}
