"""Compare offline setup through independently selected, isolated native executables."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from isolate import Workspace


def digest(data):
    return hashlib.sha256(data).hexdigest()


def observe(installation, relay):
    result = {}
    with Workspace() as cohort:
        peers = {}
        for role in ("client", "server"):
            peer = cohort.instance(role)
            peers[role] = peer
            with peer.profile.open("a") as file:
                file.write("(deny network*)\n")
            for binary in installation.iterdir():
                if binary.name.startswith("marmish") and binary.is_file() and binary.stat().st_mode & 0o111:
                    shutil.copy2(binary, peer.root / binary.name)
        ids = {}
        keys = {}

        def invoke(role, case, options, status=0, operation="setup"):
            peer = peers[role]
            command = [peer.root / "marmish", "--state-dir", peer.root / "state", "--identity", role,
                       "--key-storage", "file", "--tor", "auto", "--relay", relay, operation, *options]
            output = peer.run(command, timeout=45)
            if output.returncode != status:
                raise ValueError(f"{case} returned {output.returncode}, expected {status}")
            stdout = output.stdout
            profile = peer.root / "state" / role
            if status == 0:
                report = json.loads(stdout)
                pub = report["npub"] if operation == "setup" else ids[role]
                if pub != ids.setdefault(role, pub) or pub == ids.get("server" if role == "client" else "client"):
                    raise ValueError("unexpected peer identity")
                manifest = json.loads((profile/"profile.json").read_bytes())
                key = manifest["public_key"]
                if key != keys.setdefault(role, key) or len(key) != 64:
                    raise ValueError("public key changed")
            config_file = profile / "config.json"
            config = config_file.read_bytes()
            for label, pub in ids.items():
                stdout = stdout.replace(pub.encode(), f"<npub-{label}>".encode())
            for label, key in keys.items():
                stdout = stdout.replace(key.encode(), f"<hex-{label}>".encode())
                config = config.replace(key.encode(), f"<hex-{label}>".encode())
            if operation == "init" and report["identity"] != keys[role]:
                raise ValueError("init reopened a different signer")
            result[case] = {"status":status,"stdout":digest(stdout),"stderr":digest(output.stderr),
                            "config":digest(config),"configMode":config_file.stat().st_mode & 0o777}
            if result[case]["configMode"] != 0o600:
                raise ValueError("setup configuration mode differs")

        for role in peers:
            invoke(role, f"{role}-fresh", ["--role", role, "--no-grants"])
        for role, other, option in [("server","client","--grant"),("client","server","--trust")]:
            update = ["--update", option, ids[other]]
            invoke(role, role+"-update", update)
            invoke(role, role+"-repeat", update)
            invoke(role, role+"-invalid", ["--update", option, "bad-public-key"], 1)
            if result[role+"-invalid"]["config"] != result[role+"-update"]["config"]:
                raise ValueError("rejected input changed authorization")
        invoke("server", "init-with-grant", [], operation="init")
        invoke("server", "implicit-revoke", ["--update","--no-grants"], 1)
        invoke("server", "revoke", ["--update","--revoke",ids["client"]])
        invoke("client", "untrust", ["--update","--untrust",ids["server"]])
    if cohort.root.exists():
        raise ValueError("test workspaces survived teardown")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("original", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()
    # The public onion is a deterministic fixture address. The sandbox denies all network access.
    import base64
    key = bytes(range(32))
    checksum = hashlib.sha3_256(b".onion checksum"+key+b"\x03").digest()[:2]
    label = base64.b32encode(key+checksum+b"\x03").decode().lower()
    results = {}
    for route, relay in [("wss", "wss://127.0.0.1:9443"), ("onion", f"ws://{label}.onion/")]:
        reference = observe(args.original.resolve(), relay)
        candidate = observe(args.candidate.resolve(), relay)
        for name in reference:
            if candidate[name] != reference[name]:
                raise ValueError(f"{route}/{name} differs: {candidate[name]} vs {reference[name]}")
        results[route]=candidate
    print(json.dumps({"cases":results, "workspacesRemoved":True, "networkDenied":True,
                      "originalSha256":digest((args.original/"marmish").read_bytes()),
                      "candidateSha256":digest((args.candidate/"marmish").read_bytes())},indent=2))


if __name__ == "__main__":
    main()
