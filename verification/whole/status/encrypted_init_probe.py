"""Compare original and projected durable effects before any cross-consumer opens them."""

import argparse
import fcntl
import hashlib
import hmac
import json
import os
import platform
from pathlib import Path
import re
import signal
import shutil
import socket
import stat
import time
import subprocess
import tempfile

from file_profile_probe import material, run

ARGS = ["--key-storage", "file", "--relay", "wss://127.0.0.1:9", "--tor", "off", "init"]


def inspect(binary, database, root_key, purpose):
    assert database.read_bytes()[:16] != b"SQLite format 3\x00"
    extract = hmac.new(b"marmish/v2/storage", root_key, hashlib.sha256).digest()
    key = hmac.new(extract, purpose.encode() + b"\1", hashlib.sha256).hexdigest().encode()
    result = subprocess.run([binary, database, purpose], input=key, capture_output=True, timeout=30, check=True)
    wrong = subprocess.run([binary, database, purpose], input=(b"1" if key[0:1] == b"0" else b"0") + key[1:], capture_output=True, timeout=30)
    assert wrong.returncode != 0 and not wrong.stdout
    result = json.loads(result.stdout)
    if purpose == "journal":
        # SQLite displays presentation differences even for equivalent constraints.
        for shape in result["objects"]:
            normalized = re.sub(r'"([a-zA-Z0-9_]+)"', r'\1', shape["declaration"])
            # Remove one enclosing group only when it spans the entire CHECK.
            start = normalized.find("CHECK((")
            if start != -1:
                left = start + len("CHECK(")
                depth = 1
                for right in range(left + 1, len(normalized) - 1):
                    depth += (normalized[right] == "(") - (normalized[right] == ")")
                    if depth == 0:
                        if normalized[right + 1] == ")":
                            normalized = normalized[:left] + normalized[left+1:right] + normalized[right+1:]
                        break
            shape["declaration"] = re.sub(r"\s+", "", normalized)
    return result


def durable(root, probe, env):
    profile = root / "default"
    record, keys = material(profile, "ready")
    entries = {}
    for path in profile.rglob("*"):
        kind = path.lstat()
        relative = str(path.relative_to(profile))
        if relative.startswith("keys/"):
            relative = "keys/" + relative.split("/", 1)[1].split("-", 1)[0] + "-<reference>"
        assert kind.st_uid == os.geteuid()
        if stat.S_ISDIR(kind.st_mode):
            assert stat.S_IMODE(kind.st_mode) == 0o700
        else:
            assert stat.S_ISREG(kind.st_mode) and kind.st_nlink == 1 and stat.S_IMODE(kind.st_mode) == 0o600
        entries[relative] = oct(stat.S_IMODE(kind.st_mode))
    assert not list(Path(env["TMPDIR"]).glob("mr2-*"))
    proofs = {role: inspect(probe, profile / (role + ".db"), keys[1], role) for role in ("journal", "mdk")}
    assert proofs["journal"]["measurements"].pop("identity") == record["public_key"]
    return record, entries, proofs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("inspector", type=Path)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--generated-owner", type=Path)
    args = parser.parse_args()
    checks = []
    args.original = args.original.resolve()
    args.candidate = args.candidate.resolve()
    args.inspector = args.inspector.resolve()
    baseline = Path(__file__).with_name("macos-arm64.json" if platform.system() == "Darwin" else "linux-arm64.json")
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest(args.original) == json.loads(baseline.read_bytes())["originalBinaries"]["marmish"]
    with tempfile.TemporaryDirectory(dir="/tmp", prefix="pi-") as temp:
        root = Path(temp).resolve()
        env = {"HOME": str(root / "home"), "TMPDIR": str(root / "tmp"), "PATH": os.defpath}
        for name in ("HOME", "TMPDIR"):
            Path(env[name]).mkdir(mode=0o700)
        measured = {}
        for name, binary in (("original", args.original), ("candidate", args.candidate)):
            state = root / name
            code, output, error = run(binary, ["--state-dir", state, *ARGS], env, root)
            assert code == 0 and not error, (name, code, output, error)
            manifest, entries, proofs = durable(state, args.inspector, env)
            expected = dict(identity=manifest["public_key"], conversations=0, received=0, pending=0)
            assert output == json.dumps(expected, separators=(",", ":")) + "\n"
            measured[name] = (state, manifest, entries, proofs)
        reference, candidate = measured["original"], measured["candidate"]
        assert reference[1]["public_key"] != candidate[1]["public_key"]
        assert reference[2] == candidate[2], (reference[2], candidate[2])
        assert reference[3] == candidate[3], "schema, seed, mode or row discrepancy"
        checks.append("mandatory durable effects and counts independently checked before cross-consumption")
        if args.generated_owner:
            state, document = candidate[:2]
            runtime = Path(env["TMPDIR"]) / f'mr2-{os.geteuid()}-{document["profile_id"]}'
            relay = socket.socket()
            relay.bind(("127.0.0.1", 0))
            relay.listen()
            relay.settimeout(5)
            url = f"wss://127.0.0.1:{relay.getsockname()[1]}"
            owner = subprocess.Popen([args.generated_owner.resolve(), "--foreground", "--state-dir", state,
                                      "--key-storage","file","--tor","off","--relay",url],
                                     cwd=root, env=env, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, umask=0o077)
            peer = None
            try:
                deadline = time.monotonic() + 6
                while not (runtime / "s").is_socket():
                    assert owner.poll() is None and time.monotonic() < deadline
                    time.sleep(.02)
                peer, _ = relay.accept()
                peer.settimeout(5)
                with peer.makefile("rb") as wire:
                    assert wire.read(3) == b"\x16\x03\x01"
                status = run(args.candidate, ["--state-dir", state, "status"], env, root)
                assert status == (0, '{"ok":true,"result":{"state":"activating"},"error":null}\n', ''), status
            finally:
                if owner.poll() is None:
                    owner.send_signal(signal.SIGTERM)
                try:
                    owner.communicate(timeout=6)
                finally:
                    relay.close()
                    if peer:
                        peer.close()
                    if owner.poll() is None:
                        owner.kill()
                        owner.communicate()
            checks.append("generated owner and client consume generated profile before original initialization")
        for implementation, sample in ((args.original, candidate), (args.candidate, reference)):
            state, before = sample[:2]
            _, key_material = material(state / "default", "ready")
            code, output, error = run(implementation, ["--state-dir", state, *ARGS], env, root)
            assert code == 0 and not error, (implementation.name, code, output, error)
            assert json.loads(output)["identity"] == before["public_key"]
            after, _, proof = durable(state, args.inspector, env)
            assert after == before and proof == sample[3]
            assert material(state / "default", "ready")[1] == key_material
        checks.append("bidirectional cross-consumption retains identity, schema, keys and counts")
        for extra, expected in [
            (["--foreground"], "marmish: invalid input: foreground_operation\n"),
            (["--max-commands", "1"], "marmish: invalid input: server_limit_operation\n"),
            (["--new-session"], "marmish: invalid input: new_session_operation\n"),
        ]:
            unused = root / "unused"
            for binary in (args.original, args.candidate):
                assert (actual := run(binary, ["--state-dir", unused, *extra, *ARGS], env, root)) == (1, "", expected), (binary.name, extra, actual)
                assert not unused.exists()
        with (candidate[0] / "default" / "owner.lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for binary in (args.original, args.candidate):
                assert run(binary, ["--state-dir", candidate[0], *ARGS], env, root) == (
                    1, "", "marmish: identity: profile_in_use\n")
        checks.append("ordered invocation guards and lease denial preserve filesystem state")
        provider_diagnostics = {}
        for name, binary in (("original", args.original), ("candidate", args.candidate)):
            bad = root / (name + "-bad")
            shutil.copytree(candidate[0], bad)
            profile = bad / "default"
            with (profile / "profile.json").open() as document:
                pending = json.load(document)
            pending["phase"] = "initializing"
            (profile / "profile.json").write_text(json.dumps(pending, separators=(",", ":")))
            (profile / "journal.db").write_bytes(b"corrupt encrypted database")
            encrypted_before = digest(profile / "mdk.db")
            private_temp = root / (name + "-tmp")
            private_temp.mkdir(mode=0o700)
            actual = run(binary, ["--state-dir", bad, *ARGS], {**env, "TMPDIR": str(private_temp)}, root)
            code, stdout, stderr = actual
            message = stderr.splitlines()
            assert (code, stdout, message[-1:]) == (1, "", ["marmish: storage: state_preflight"]), (name, actual)
            # Some native SQLCipher builds log a timestamped provider trace before the named application error.
            trace = []
            for line in message[:-1]:
                match = re.fullmatch(r"\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d+: ERROR CORE (.+)", line)
                assert match, (name, line)
                trace.append(match.group(1))
            provider_diagnostics[name] = trace
            assert digest(profile / "mdk.db") == encrypted_before
            assert json.loads((profile / "profile.json").read_bytes()) == pending
            assert (profile / "journal.db").read_bytes() == b"corrupt encrypted database"
        assert provider_diagnostics["original"] == provider_diagnostics["candidate"]
        checks.append("corrupt encrypted journal blocks ready transition without replacing either store")
        receipt = {"originalRevision":"af54339d250290490d62d4475a77539b7e739071",
                   "generatedOwnerBinarySha256":digest(args.generated_owner.resolve()) if args.generated_owner else None,
                   "originalBinarySha256":digest(args.original), "candidateBinarySha256":digest(args.candidate),
                   "inspectorSha256":digest(args.inspector), "frameworkRevision": args.framework_revision, "platform": platform.system() + "-" + platform.machine(),
                   "checks": checks, "corruptJournalProviderStderr": provider_diagnostics,
                   "files": reference[2], "stores": {
                       purpose: {"version": value["version"], "objects": len(value["objects"]),
                                 "initialRows": {obj["name"]: obj["rows"] for obj in value["objects"] if obj["rows"]}}
                       for purpose, value in reference[3].items()}}
        args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
