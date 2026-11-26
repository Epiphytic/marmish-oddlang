"""Compare status endpoint discovery on fresh same-UID private profiles and a live daemon."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import stat
import subprocess
import sys
import tempfile
import time

from observe import ACTIVATING, STATUS, expect, frame, result

HERE = Path(__file__).resolve().parent
PROFILE_ID = "11" * 16
READY_KEY = "22" * 32


def private(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.chmod(0o700)


def encoded(value):
    return json.dumps(value, separators=(",", ":")).encode()


def document(path, value):
    path.write_bytes(value if isinstance(value, bytes) else encoded(value))
    path.chmod(0o600)


def fixture(root, env, name="default", state=None):
    base = state or root / "state"
    private(base)
    profile = base / name
    private(profile)
    manifest = {
        "format": 2, "profile_id": PROFILE_ID, "name": name, "transaction": "33" * 16,
        "phase": "ready", "backend": "files", "nostr_ref": "44" * 32, "sqlcipher_ref": "55" * 32,
        "device_slot": "66" * 16, "public_key": READY_KEY,
    }
    runtime = Path(env["TMPDIR"]) / f"mr2-{os.geteuid()}-{PROFILE_ID}"
    private(runtime)
    binding = {"format": 2, "profile_id": PROFILE_ID, "root": str(profile.resolve())}
    document(profile / "profile.json", manifest)
    document(runtime / "binding.json", binding)
    return base, profile, runtime, manifest, binding


def native(binary, args, env, directory, path=None):
    if path is None:
        child = subprocess.Popen([binary, *args, "status"], cwd=directory, env=env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, umask=0o077)
        return result(child)

    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(str(path))
        listener.listen(1)
        listener.settimeout(3)
        with subprocess.Popen([binary, *args, "status"], cwd=directory, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, umask=0o077) as child:
            try:
                conn, _ = listener.accept()
                with conn:
                    conn.settimeout(3)
                    request = bytearray()
                    while data := conn.recv(128):
                        request.extend(data)
                        assert len(request) <= len(frame(STATUS))
                    expect(bytes(request), frame(STATUS), "discovered endpoint request")
                    conn.sendall(frame(b'{"ok":true,"result":{"state":"fixture"}}'))
                return result(child)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.communicate()
                path.unlink()


def live(daemon, binaries, baseline, env, root):
    with socket.socket() as relay:
        relay.bind(("127.0.0.1", 0))
        relay.listen()
        url = f"wss://127.0.0.1:{relay.getsockname()[1]}"
        owner = subprocess.Popen([daemon, "--foreground", "--state-dir", baseline, "--key-storage",
                                  "file", "--tor", "off", "--relay", url], stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, env=env,
                                 cwd=root, umask=0o077)
        try:
            deadline = time.monotonic() + 7
            while not any(p.is_socket() for p in Path(env["TMPDIR"]).glob("mr2-*/s")):
                if owner.poll() is not None or time.monotonic() >= deadline:
                    raise ValueError("original daemon failed to expose a private runtime socket")
                time.sleep(0.01)
            rows = [native(binary, ["--state-dir", baseline], env, root) for binary in binaries]
            rows += [native(binary, [], env, root) for binary in binaries]
            for row in rows:
                expect(row, {"exit": 0, "stdout": ACTIVATING, "stderr": ""}, "discovered live daemon")
        finally:
            if owner.poll() is None:
                owner.terminate()
            shutdown = result(owner)
        expect(shutdown["exit"], 0, "daemon SIGTERM")
        cleanup = json.loads(shutdown["stdout"])
        fields = ("engine_stop_complete", "mdk_stop_complete", "runtime_cleanup_complete")
        for key in fields:
            expect(cleanup[key], True, key)
    return {"stateDirectory": rows[0], "defaultDirectory": rows[-1],
            "shutdown": {key: cleanup[key] for key in fields}}


def home_state(home):
    return home / ("Library/Application Support/Marmish/v2" if sys.platform == "darwin" else ".local/share/marmish/v2")


def environment(root):
    home, runtime = root / "h", root / "t"
    private(home)
    private(runtime)
    return {"HOME": str(home), "TMPDIR": str(runtime), "LANG": "C", "PATH": os.defpath}


def contents(root):
    rows = {}
    for path in root.rglob("*"):
        metadata = path.lstat()
        mode = metadata.st_mode
        if stat.S_ISREG(mode):
            data = sha(path)
        elif stat.S_ISLNK(mode):
            data = os.readlink(path)
        else:
            data = None
        rows[path.relative_to(root)] = (mode, metadata.st_nlink, data)
    return rows


def controlled(binaries):
    cases = (
        "state_relative", "custom_profile", "home_default", "xdg_absolute", "xdg_relative",
        "xdg_without_home", "foreground_precedes_discovery", "limit_precedes_discovery",
        "new_session_precedes_discovery", "route_precedes_discovery",
        "no_home", "no_home_explicit_socket", "explicit_bypasses_files", "config_ignored",
        "invalid_profile_precedes_missing_base", "missing_base", "base_mode", "base_symlink",
        "profile_mode", "profile_symlink", "missing_manifest", "manifest_mode", "manifest_symlink",
        "manifest_hardlink", "manifest_fifo", "manifest_duplicate", "manifest_unknown", "manifest_large",
        "manifest_format_precedence", "manifest_unsigned_width", "ready_needs_key", "initializing_no_key",
        "missing_runtime", "runtime_mode", "missing_binding", "binding_mode", "binding_duplicate",
        "binding_profile_mismatch", "binding_canonical_root", "binding_root_mismatch",
        "binding_bounded_prefix", "socket_too_long", "absent_socket",
    )
    outcomes = {}
    for case in cases:
        with tempfile.TemporaryDirectory(prefix="ocd-", dir="/tmp") as name:
            root = Path(name).resolve()
            env = environment(root)
            if case == "socket_too_long":
                env["TMPDIR"] = str(root / ("t" * 85))
            profile_name = "lab_2" if case == "custom_profile" else "default"
            if case in ("home_default", "xdg_relative") or (case == "xdg_absolute" and sys.platform == "darwin"):
                selected = home_state(Path(env["HOME"]))
            elif case in ("xdg_absolute", "xdg_without_home"):
                selected = root / "x/marmish/v2"
            else:
                selected = root / "state"
            state, profile, runtime, manifest, binding = fixture(root, env, profile_name, selected)
            manifest_file, binding_file = profile / "profile.json", runtime / "binding.json"
            args = ["--state-dir", state]
            path = None
            match case:
                case "state_relative":
                    args = ["--state-dir", state.relative_to(root)]
                    path = runtime / "s"
                case "custom_profile":
                    args += ["--identity", profile_name]
                    path = runtime / "s"
                case "home_default" | "xdg_absolute" | "xdg_relative":
                    args = []
                    if case != "home_default":
                        env["XDG_DATA_HOME"] = str(root / "x") if case == "xdg_absolute" else "relative-x"
                    path = runtime / "s"
                case "xdg_without_home":
                    env.pop("HOME")
                    env["XDG_DATA_HOME"] = str(root / "x")
                    args = []
                    if sys.platform == "linux":
                        path = runtime / "s"
                case "foreground_precedes_discovery" | "limit_precedes_discovery" | "new_session_precedes_discovery" | "route_precedes_discovery":
                    env.pop("HOME")
                    args = {
                        "foreground_precedes_discovery": ["--foreground", "--max-commands", "1"],
                        "limit_precedes_discovery": ["--max-commands", "1", "--new-session"],
                        "new_session_precedes_discovery": ["--new-session", "--tor", "invalid"],
                        "route_precedes_discovery": ["--tor", "invalid"],
                    }[case]
                case "no_home" | "no_home_explicit_socket":
                    env.pop("HOME")
                    args = ["--socket", runtime / "s"] if case.endswith("_socket") else []
                case "explicit_bypasses_files":
                    manifest_file.unlink()
                    state.chmod(0o755)
                    args += ["--identity", "invalid/name", "--socket", runtime / "s"]
                    path = runtime / "s"
                case "config_ignored":
                    document(profile / "config.json", b"not a document")
                    path = runtime / "s"
                case "invalid_profile_precedes_missing_base":
                    args = ["--state-dir", root / "absent", "--identity", "invalid/name"]
                case "missing_base":
                    args = ["--state-dir", root / "absent"]
                case "base_mode":
                    state.chmod(0o755)
                case "base_symlink":
                    (root / "alias").symlink_to(state, target_is_directory=True)
                    args = ["--state-dir", root / "alias"]
                case "profile_mode":
                    profile.chmod(0o755)
                case "profile_symlink":
                    moved = profile.with_name("other")
                    profile.rename(moved)
                    profile.symlink_to(moved, target_is_directory=True)
                case "missing_manifest":
                    manifest_file.unlink()
                case "manifest_mode":
                    manifest_file.chmod(0o644)
                case "manifest_symlink":
                    moved = manifest_file.with_name("other")
                    manifest_file.rename(moved)
                    manifest_file.symlink_to(moved)
                case "manifest_hardlink":
                    os.link(manifest_file, profile / "other")
                case "manifest_fifo":
                    manifest_file.unlink()
                    os.mkfifo(manifest_file, 0o600)
                case "manifest_duplicate":
                    document(manifest_file, b'{"format":2,' + encoded(manifest)[1:])
                case "manifest_unknown":
                    document(manifest_file, {**manifest, "extra": 0})
                case "manifest_large":
                    document(manifest_file, encoded(manifest) + b" " * 8192)
                case "manifest_format_precedence":
                    document(manifest_file, {**manifest, "format": 3, "transaction": "invalid"})
                case "manifest_unsigned_width":
                    document(manifest_file, {**manifest, "format": 256})
                case "ready_needs_key":
                    document(manifest_file, {**manifest, "public_key": None})
                case "initializing_no_key":
                    document(manifest_file, {**manifest, "phase": "initializing", "public_key": None})
                    path = runtime / "s"
                case "missing_runtime":
                    runtime.rename(runtime.with_name("other"))
                case "runtime_mode":
                    runtime.chmod(0o755)
                case "missing_binding":
                    binding_file.unlink()
                case "binding_mode":
                    binding_file.chmod(0o644)
                case "binding_duplicate":
                    document(binding_file, b'{"format":2,' + encoded(binding)[1:])
                case "binding_profile_mismatch":
                    document(binding_file, {**binding, "profile_id": "88" * 16})
                case "binding_canonical_root":
                    document(binding_file, {**binding, "root": binding["root"] + "/."})
                    path = runtime / "s"
                case "binding_root_mismatch":
                    document(binding_file, {**binding, "root": "/unrelated"})
                case "binding_bounded_prefix":
                    document(binding_file, encoded(binding) + b" " * 8193 + b"ignored")
                    path = runtime / "s"
                case "socket_too_long" | "absent_socket":
                    pass
                case _:
                    raise ValueError(case)
            before = contents(root)
            rows = []
            for binary in binaries:
                rows.append(native(binary, args, env, root, path))
                expect(contents(root), before, f"private file contents after {case}")
            if len(rows) == 2:
                expect(rows[1], rows[0], case)
            outcomes[case] = rows[0]
    return outcomes


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original_bins", type=Path)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--framework-revision")
    args = parser.parse_args()
    original = args.original_bins.resolve()
    baseline = "macos-arm64.json" if sys.platform == "darwin" else "linux-arm64.json"
    reference = json.loads((HERE / baseline).read_bytes())
    expect({name: sha(original / name) for name in ("marmish", "marmishd")},
           {name: reference["originalBinaries"][name] for name in ("marmish", "marmishd")}, "original binary hashes")
    binaries = [original / "marmish"]
    if args.candidate:
        binaries.append(args.candidate.resolve())
    with tempfile.TemporaryDirectory(prefix="oclive-", dir="/tmp") as name:
        root = Path(name).resolve()
        env = environment(root)
        state = home_state(Path(env["HOME"]))
        state.parent.mkdir(parents=True)
        live_result = live(original / "marmishd", binaries, state, env, root)
    controls = controlled(binaries)
    body = {
        "platform": f"{platform.system()}-{platform.machine()}",
        "originalRevision": reference["originalRevision"],
        "originalBinaries": reference["originalBinaries"],
        "candidateSha256": sha(binaries[1]) if len(binaries) == 2 else None,
        "frameworkRevision": args.framework_revision,
        "corpusSha256": sha(Path(__file__)),
        "live": live_result,
        "controlled": controls,
        "fixturePathsAndContentsUnchanged": True,
    }
    args.receipt.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n")
    print(f"Observed {len(controls)} fresh discovery cases and real daemon status")


if __name__ == "__main__":
    main()
