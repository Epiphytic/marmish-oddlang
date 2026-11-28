"""Qualify pending file-backed credential preparation without an init success claim."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
PRIME = 2**256 - 2**32 - 977
BASE = (
    0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
    0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8,
)


def expect(actual, expected, case):
    if actual != expected:
        raise ValueError(f"{case}: expected {expected!r}, observed {actual!r}")


def sha(value):
    return hashlib.sha256(value).hexdigest()


def public_key(secret):
    value = int.from_bytes(secret, "big")
    assert 0 < value < ORDER, "invalid generated scalar"

    def add(left, right):
        if left is None:
            return right
        if right is None:
            return left
        x1, y1 = left
        x2, y2 = right
        if x1 == x2 and (y1 + y2) % PRIME == 0:
            return None
        if x1 == x2:
            slope = (3 * x1 * x1 * pow(2 * y1, -1, PRIME)) % PRIME
        else:
            slope = ((y2 - y1) * pow(x2 - x1, -1, PRIME)) % PRIME
        x3 = (slope * slope - x1 - x2) % PRIME
        return x3, (slope * (x1 - x3) - y1) % PRIME

    result, point = None, BASE
    while value:
        if value & 1:
            result = add(result, point)
        point = add(point, point)
        value >>= 1
    return f"{result[0]:064x}"


def run(binary, args, env, root):
    process = subprocess.run([binary, *args], cwd=root, env=env, stdin=subprocess.DEVNULL,
                             capture_output=True, timeout=30, umask=0o077)
    return process.returncode, process.stdout.decode(), process.stderr.decode()


def snapshot(profile):
    rows = {}
    for path in profile.rglob("*"):
        relative = str(path.relative_to(profile))
        meta = path.lstat()
        rows[relative] = (meta.st_mode, meta.st_uid, meta.st_nlink, meta.st_ino,
                          sha(path.read_bytes()) if stat.S_ISREG(meta.st_mode) else None)
    return rows


def material(profile, phase):
    path = profile / "profile.json"
    record = json.loads(path.read_bytes())
    fields = ("format", "profile_id", "name", "transaction", "phase", "backend",
              "nostr_ref", "sqlcipher_ref", "device_slot", "public_key")
    expect(tuple(record), fields, "manifest field order")
    expect((record["format"], record["name"], record["phase"], record["backend"]),
           (2, profile.name, phase, "files"), "profile identity and phase")
    widths = {"profile_id": 16, "transaction": 16, "nostr_ref": 32, "sqlcipher_ref": 32,
              "device_slot": 16, "public_key": 32}
    for field, width in widths.items():
        text = record[field]
        assert len(bytes.fromhex(text)) == width and text == text.lower(), field
    assert record["nostr_ref"] != record["sqlcipher_ref"]
    signing = profile / "keys" / ("nostr-" + record["nostr_ref"])
    storage = profile / "keys" / ("sqlcipher-" + record["sqlcipher_ref"])
    keys = (signing.read_bytes(), storage.read_bytes())
    assert all(len(key) == 32 for key in keys) and keys[0] != keys[1]
    expect(public_key(keys[0]), record["public_key"], "independent x-only public key")
    for item in (profile, profile / "keys", profile / "arti-state", profile / "arti-cache"):
        meta = item.lstat()
        assert stat.S_ISDIR(meta.st_mode) and stat.S_IMODE(meta.st_mode) == 0o700 and meta.st_uid == os.geteuid()
    for item in (path, profile / "owner.lock", signing, storage):
        meta = item.lstat()
        assert stat.S_ISREG(meta.st_mode) and stat.S_IMODE(meta.st_mode) == 0o600
        assert meta.st_nlink == 1 and meta.st_uid == os.geteuid()
    return record, keys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    original, candidate = args.original.resolve(), args.candidate.resolve()
    baseline = HERE / ("macos-arm64.json" if platform.system() == "Darwin" else "linux-arm64.json")
    original_reference = json.loads(baseline.read_bytes())
    expect(sha(original.read_bytes()), original_reference["originalBinaries"]["marmish"], "pinned original CLI")
    with tempfile.TemporaryDirectory(prefix="opp-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        state, home, tmp = (root / name for name in ("state", "h", "t"))
        for item in (home, tmp):
            item.mkdir(mode=0o700)
        env = {"HOME": str(home), "TMPDIR": str(tmp), "PATH": os.defpath, "LANG": "C"}

        def prepare(name, base=state):
            return run(candidate, ["--state-dir", base, "--identity", name, "--key-storage", "file", "prepare"], env, root)

        def initialize(name):
            return run(original, ["--state-dir", state, "--identity", name, "--key-storage", "file",
                                  "--relay", "wss://127.0.0.1:9", "--tor", "off", "init"], env, root)

        first, second = state / "first", state / "other"
        results = (prepare("first"), prepare("other"))
        assert all(row[0] == 0 and row[2] == "" for row in results), [(r[0], r[2]) for r in results]
        first_record, first_keys = material(first, "initializing")
        second_record, second_keys = material(second, "initializing")
        for field in ("profile_id", "transaction", "nostr_ref", "sqlcipher_ref", "device_slot", "public_key"):
            assert first_record[field] != second_record[field], field
        assert set(first_keys).isdisjoint(second_keys)
        expect(json.loads(results[0][1]), {"profile": "first", "identity": first_record["public_key"],
               "profile_phase": "initializing", "credential_preparation_complete": True}, "partial report")
        before = snapshot(first)
        initial_files = set(before)
        expected_dirs = {"owner.lock", "profile.json", "keys", "arti-cache", "arti-state"}
        expect(initial_files, expected_dirs | {"keys/nostr-" + first_record["nostr_ref"],
               "keys/sqlcipher-" + first_record["sqlcipher_ref"]}, "partial file set")
        expect(prepare("first"), results[0], "idempotent pending profile")
        expect(snapshot(first), before, "pending profile contents and inodes")
        with (first / "owner.lock").open("r") as owner:
            fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
            expect(prepare("first"), (1, "", "marmish: identity: profile_in_use\n"), "exclusive lease")
        attested = initialize("first")
        assert attested[0] == 0 and not attested[2], (attested[0], attested[2])
        expect(json.loads(attested[1]), {"identity": first_record["public_key"], "conversations": 0,
                                        "received": 0, "pending": 0}, "consumer initializes from prepared profile")
        ready, ready_keys = material(first, "ready")
        expect(ready, {**first_record, "phase": "ready"}, "original promotes same manifest")
        expect(ready_keys, first_keys, "original consumes original-format private keys")
        stores = [first / "mdk.db", first / "journal.db"]
        assert all(path.is_file() and path.stat().st_size > 0 for path in stores)
        fresh = initialize("native")
        assert fresh[0] == 0 and not fresh[2], (fresh[0], fresh[2])
        native = state / "native"
        native_record, _ = material(native, "ready")
        before = snapshot(native)
        prepared = prepare("native")
        assert prepared[0] == 0 and not prepared[2], (prepared[0], prepared[2])
        expect(json.loads(prepared[1])["identity"], native_record["public_key"], "generated consumes original keys")
        expect(snapshot(native), before, "ready original files unchanged")
        key = second / "keys" / ("nostr-" + second_record["nostr_ref"])
        key.write_bytes(b"bad")
        snap = snapshot(second)
        expect(prepare("other"), (1, "", "marmish: identity: invalid_secret\n"), "invalid length")
        expect(snapshot(second), snap, "invalid key unchanged")
        key.write_bytes((1).to_bytes(32, "big"))
        snap = snapshot(second)
        expect(prepare("other"), (1, "", "marmish: identity: identity_mismatch\n"), "mismatched identity")
        expect(snapshot(second), snap, "mismatched key unchanged")
        key.chmod(0o644)
        expect(prepare("other"), (1, "", "marmish: identity: unsafe_profile_file\n"), "unsafe key mode")
        bad = root / "bad"
        bad.mkdir(mode=0o755)
        expect(prepare("safe", bad), (1, "", "marmish: identity: unsafe_profile_path\n"), "unsafe state")
    receipt = {
        "platform": f"{platform.system()}-{platform.machine()}",
        "frameworkRevision": args.framework_revision, "originalRevision": original_reference["originalRevision"],
        "originalBinarySha256": sha(original.read_bytes()), "candidateBinarySha256": sha(candidate.read_bytes()),
        "scope": "pending credential preparation",
        "checks": ["private directories and single-link files", "two independent random profiles",
                   "independent x-only identity", "repeat preserves pending contents and inodes", "exclusive lease",
                   "original creates stores from prepared identity", "generated reads original ready credentials",
                   "invalid key length", "mismatched public identity", "unsafe key and state modes"],
        "storesWereAbsentBeforeOriginalInit": True,
    }
    args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print("Qualified pending credential preparation and both profile directions")


if __name__ == "__main__":
    main()
