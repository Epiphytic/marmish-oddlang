"""Compare the native per-profile provider lifetime on local held TLS lanes."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE.parent / folder) for folder in ("foreground-owner", "relay", "status")]
from qualify import LOCKED, SUCCESS, PreparedProfile, call, held_relay, observation, rejected_certificate, relay_options
from owner_tls import certificate
from encrypted_init_probe import durable

PIN = "af54339d250290490d62d4475a77539b7e739071"
STORES = ("journal.db", "mdk.db")
TERMINAL = {
    "schema": 1, "engine_quiesced": False, "engine_stop_complete": True,
    "mdk_stop_complete": True, "runtime_cleanup_complete": True,
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_exact(peer, count):
    value = bytearray()
    while len(value) < count:
        data = peer.recv(count - len(value))
        if not data:
            raise AssertionError("unexpected TLS peer EOF")
        value.extend(data)
    return value


def file_handles(pid):
    if sys.platform == "linux":
        names = set()
        for entry in Path(f"/proc/{pid}/fd").iterdir():
            try:
                names.add(os.readlink(entry))
            except FileNotFoundError:
                continue
        return names
    output = subprocess.run(["/usr/sbin/lsof", "-a", "-p", str(pid), "-Fn"],
                            capture_output=True, text=True, timeout=6, check=True)
    return {line.removeprefix("n") for line in output.stdout.splitlines() if line.startswith("n")}


def release_check(profile):
    with (profile.path / "owner.lock").open("rb") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)


def signal_trial(owner, competitor, clients, profile, common, root, runtime, env, sig):
    with held_relay() as relay, held_relay() as blocked:
        command = [owner, "--foreground", *common, *relay_options(relay)]
        child = subprocess.Popen(command, cwd=root, env=env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, umask=0o077)
        try:
            deadline = time.monotonic() + 8
            while not (runtime / "s").is_socket():
                assert child.poll() is None and time.monotonic() < deadline, "missing status listener"
                time.sleep(0.01)
            peer, _ = relay.accept()
            with peer:
                peer.settimeout(8)
                header = read_exact(peer, 5)
                assert header[:3] == b"\x16\x03\x01"
                payload = read_exact(peer, int.from_bytes(header[3:], "big"))
                assert payload[:1] == b"\x01", "expected a real ClientHello"
                handles = file_handles(child.pid)
                assert all(str(profile.path / leaf) in handles for leaf in STORES), handles
                assert all(call([client, *common, "status"], env, root) == SUCCESS for client in clients)
                contested = call([competitor, "--foreground", *common, *relay_options(blocked)], env, root)
                assert contested == LOCKED
                child.send_signal(sig)
                out, err = child.communicate(timeout=10)
                # A ClientHello can span records. Drain until the owned dial really closes.
                drained = 0
                while piece := peer.recv(4096):
                    drained += len(piece)
                    assert drained <= 65536
            result = observation(child.returncode, out, err)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()
    assert not runtime.exists()
    release_check(profile)
    return {"heldFiles": list(STORES), "tlsRecordPrefix": header[:3].hex(), "tlsEof": True,
            "status": SUCCESS, "leaseContender": contested, "terminal": result}


def early_rejection(label, owner, profile, common, root, runtime, env):
    leaf = "mdk.db" if label == "corrupt-account" else "journal.db"
    file = profile.path / leaf
    original = file.read_bytes()
    if label == "unsafe-journal":
        file.chmod(0o644)
        expected = "marmish: identity: unsafe_profile_file"
    else:
        file.write_bytes(bytes([original[0] ^ 0xFF]) + original[1:])
        expected = "marmish: storage: state_preflight"
    observed_before = {name: sha(profile.path / name) for name in STORES}
    try:
        with held_relay() as relay:
            result = call([owner, "--foreground", *common, *relay_options(relay)], env, root)
            relay.setblocking(False)
            try:
                connection, _ = relay.accept()
            except BlockingIOError:
                pass
            else:
                connection.close()
                raise AssertionError("preflight failure attempted a relay connection")
        assert result["exit"] == 1 and not result["stdout"], result
        lines = result["stderr"].splitlines()
        assert lines[-1:] == [expected], result
        messages = []
        for line in lines[:-1]:
            match = re.fullmatch(r"\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d+: ERROR CORE (.+)", line)
            assert match, line
            messages.append(match[1])
        assert sorted(path.name for path in runtime.iterdir()) == ["binding.json"]
        assert stat.S_IMODE(runtime.stat().st_mode) == 0o700
        binding = runtime / "binding.json"
        assert stat.S_IMODE(binding.stat().st_mode) == 0o600
        assert json.loads(binding.read_bytes()) == {"format": 2, "profile_id": profile.name_token,
                                                     "root": str(profile.path)}
        assert observed_before == {name: sha(profile.path / name) for name in STORES}
        release_check(profile)
        return {"exit": 1, "stderr": expected, "providerTrace": messages,
                "unchangedStores": True, "boundWithoutSocket": True, "leaseReleased": True}
    finally:
        if label == "unsafe-journal":
            file.chmod(0o600)
        else:
            file.write_bytes(original)


def observe(binaries, candidate, inspector, root, initializer):
    root.mkdir()
    env = {"HOME": str(root / "h"), "TMPDIR": str(root / "t"), "PATH": os.defpath}
    for value in ("HOME", "TMPDIR"):
        Path(env[value]).mkdir(mode=0o700)
    state = root / "s"
    common = ["--state-dir", state]
    reference = binaries / "marmish"
    original_owner = binaries / "marmishd"
    owners = (original_owner, candidate / "marmishd")
    clients = (reference, candidate / "marmish")
    with held_relay() as relay:
        provision = call([initializer, *common, *relay_options(relay), "init"], env, root)
    assert provision["exit"] == 0 and not provision["stderr"], provision
    profile = PreparedProfile(state / "default")
    runtime = Path(env["TMPDIR"]) / f"mr2-{os.geteuid()}-{profile.name_token}"
    credentials = [profile.credential("nostr", "nostr_ref"), profile.credential("sqlcipher", "sqlcipher_ref")]
    assert json.loads(provision["stdout"]) == {
        "identity": profile.document["public_key"], "conversations": 0, "received": 0, "pending": 0,
    }
    before_keys = list(map(sha, credentials))
    initial = durable(state, inspector, env)

    def check_durable():
        assert durable(state, inspector, env) == initial
        assert list(map(sha, credentials)) == before_keys

    reports = {}
    for label, sig in (("sigterm", signal.SIGTERM), ("sigint", signal.SIGINT)):
        first, compared = (
            signal_trial(owner, competitor, clients, profile, common, root, runtime, env, sig)
            for owner, competitor in ((owners[0], owners[1]), (owners[1], owners[0]))
        )
        assert compared == first, (label, first, compared)
        assert first["terminal"] == {"exit": 0, "stderr": "",
                                     "stdout": json.dumps(TERMINAL, separators=(",", ":")) + "\n"}
        check_durable()
        reports[label] = first
    with tempfile.TemporaryDirectory(dir=root, prefix="keys-") as directory:
        tls = certificate(Path(directory))
        for owner in owners:
            terminal = rejected_certificate(owner, common, root, runtime, env, tls)
            assert terminal["stdout"] == reports["sigterm"]["terminal"]["stdout"]
            check_durable()
        reports["untrustedCertificate"] = terminal
    for label in ("corrupt-journal", "corrupt-account", "unsafe-journal"):
        first = early_rejection(label, owners[0], profile, common, root, runtime, env)
        (runtime / "binding.json").unlink()
        runtime.rmdir()
        second = early_rejection(label, owners[1], profile, common, root, runtime, env)
        assert first == second, (label, first, second)
        trial = signal_trial(owners[1], owners[0], clients, profile, common, root, runtime, env, signal.SIGTERM)
        assert trial == reports["sigterm"], "preflight retry left unusable runtime"
        check_durable()
        reports[label] = first
    control = profile.path / "config.json"
    control.write_bytes(b"local lifecycle scope control")
    try:
        with held_relay() as listener:
            skipped = call([owners[1], "--foreground", *common, *relay_options(listener)], env, root)
        assert skipped == {"exit": 125, "stdout": "",
                           "stderr": "marmish: foreground status owner requires explicit state and a prepared file profile\n"}
        assert not runtime.exists() and control.read_bytes() == b"local lifecycle scope control"
        release_check(profile)
    finally:
        control.unlink()
    reports["configuredProfileRejected"] = skipped
    return reports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binaries", "candidate", "inspector", "receipt"):
        parser.add_argument(name, type=lambda name: Path(name).resolve())
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    baseline = HERE.parent / "status" / ("linux-arm64.json" if sys.platform == "linux" else "macos-arm64.json")
    pinned = json.loads(baseline.read_bytes())["originalBinaries"]
    for name in ("marmish", "marmishd"):
        assert sha(args.binaries / name) == pinned[name], name
    with tempfile.TemporaryDirectory(prefix="mvl-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        lanes = {
            label: observe(args.binaries, args.candidate, args.inspector, root / label, initializer / "marmish")
            for label, initializer in (("originalInit", args.binaries), ("generatedInit", args.candidate))
        }
        assert lanes["originalInit"] == lanes["generatedInit"]
    result = {"originalRevision": PIN, "frameworkRevision": args.framework_revision,
              "platform": f"{platform.system()}-{platform.machine()}", "observations": lanes,
              "originalBinaries": {name: pinned[name] for name in ("marmish", "marmishd")},
              "candidateBinaries": {name: sha(args.candidate / name) for name in ("marmish", "marmishd")},
              "inspectorSha256": sha(args.inspector)}
    with args.receipt.open("x") as stream:
        stream.write(json.dumps(result, sort_keys=True, indent=2) + "\n")


if __name__ == "__main__":
    main()
