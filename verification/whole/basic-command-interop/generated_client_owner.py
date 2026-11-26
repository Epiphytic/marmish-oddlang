#!/usr/bin/env python3
"""Compare a generated client handoff with the pinned upstream local receipt."""

import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile

from client_owner_preflight import (
    CAPACITY, CASES, LOCAL_ACCOUNT, NPUB, PREPARE, READY, RELAY, STATUS, TOOLS,
    config, exchange, observe, read, sha,
)


HERE = Path(__file__).resolve().parent
REFERENCE = HERE / "upstream-local-owner-aarch64-apple-darwin.json"
SCOPED_CASES = (
    "foregroundSubmit", "detachedSubmit", "malformedPrepareReply",
    "readyForegroundSubmit", "readyDetachedSubmit",
)


def confined(binary, workspace, path):
    sandbox, option, policy = TOOLS.isolated(workspace, (binary.parent,))
    policy += f"(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))"
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    env = {"HOME": str(workspace), "TMPDIR": str(workspace), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
    return [sandbox, option, policy, binary], env


def publish_test_endpoint(work, state):
    profile = state / "default"
    tag = "75" * 16
    runtime = work / f"mr2-{os.geteuid()}-{tag}"
    runtime.mkdir(mode=0o700)
    for path, body in (
        (profile / "profile.json",
         {"format": 2, "profile_id": tag, "name": "default", "transaction": "79" * 16,
          "phase": "ready", "backend": "files", "nostr_ref": "41" * 32, "sqlcipher_ref": "42" * 32,
          "device_slot": "8a" * 16, "public_key": LOCAL_ACCOUNT}),
        (runtime / "binding.json", {"format": 2, "profile_id": tag, "root": str(profile)}),
    ):
        path.write_text(json.dumps(body))
        path.chmod(0o600)
    return runtime / "s"


def owner_discovery(binary, work, *, canonical_relays=False):
    state = work / "state"
    config(state)
    prepare = PREPARE
    if canonical_relays:
        path = work / "owner"
        alternate = "wss://localhost:31338/"
        file = state / "default" / "config.json"
        document = read(file)
        document["relays"] = [alternate, RELAY, alternate]
        file.write_text(json.dumps(document))
        options = ["--socket", path, "--relay", alternate, "--relay", RELAY]
        prepare = PREPARE.replace(RELAY.encode()+b'"', RELAY.encode()+b'","'+alternate.encode()+b'"')
    else:
        path = publish_test_endpoint(work, state)
        options = []
    launch, env = confined(binary, work, path)
    argv = [*launch, "--state-dir", state, "--no-autostart", "--stdin-none", *options,
            NPUB, "--", "/usr/bin/true"]
    with socket.socket(socket.AF_UNIX) as owner:
        owner.bind(str(path))
        owner.listen()
        owner.settimeout(3)
        with subprocess.Popen(argv, cwd=work, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
            try:
                for request, reply in ((STATUS, READY), (prepare, CAPACITY)):
                    exchange(owner, request, reply)
                stdout, stderr = process.communicate(timeout=3)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=3)
    expected = b"marmish: command_room_capacity\n"
    if (process.returncode, stdout, stderr) != (255, b"", expected):
        raise ValueError(f"profile endpoint handoff differs: {process.returncode}, {stdout!r}, {stderr!r}")
    return {"exit": process.returncode, "stderrHex": stderr.hex(),
            "profileEndpointVerified": not canonical_relays, "canonicalRelaySetVerified": canonical_relays}


def reject(binary, workspace, failure, change, extra=(), *, discover=False, input_bytes=None):
    state = workspace / "state"
    config(state)
    document = state / "default" / "config.json"
    change(document)
    path = publish_test_endpoint(workspace, state) if discover else workspace / "owner"
    launch, env = confined(binary, workspace, path)
    with socket.socket(socket.AF_UNIX) as owner:
        owner.bind(str(path))
        owner.listen()
        owner.setblocking(False)
        socket_flags = [] if discover else ["--socket", path]
        stdin_flag = "--stdin" if input_bytes is not None else "--stdin-none"
        command = [*launch, "--state-dir", state, *socket_flags,
                   "--no-autostart", stdin_flag, *extra, NPUB, "--", "/usr/bin/true"]
        call = subprocess.run(command, cwd=workspace, env=env, input=input_bytes or b"",
                              capture_output=True, timeout=3)
        try:
            unsolicited, _ = owner.accept()
        except BlockingIOError:
            pass
        else:
            with unsolicited:
                raise ValueError("candidate contacted owner despite rejected client authorization")
    if (call.returncode, call.stdout, call.stderr) != failure:
        raise ValueError(f"unexpected authorization diagnostic: {call.returncode}, {call.stdout!r}, {call.stderr!r}")
    return {"exit": call.returncode, "stderrHex": call.stderr.hex(), "ownerContacted": False}


def untrust(document):
    data = read(document)
    data["trusted_remotes"] = []
    document.write_text(json.dumps(data))


def duplicate_member(document):
    data = document.read_text()
    document.write_text(data.replace('"version": 1', '"version": 1,"version": 1', 1))


def symlink(document):
    retained = document.with_name("retained.json")
    document.rename(retained)
    document.symlink_to(retained)


def hardlink(document):
    os.link(document, document.with_name("alias.json"))


def invalid_onion(document):
    data = read(document)
    data["tor"] = "auto"
    data["relays"] = ["ws://" + "a" * 56 + ".onion/"]
    document.write_text(json.dumps(data))


def verify(binary, expected_digest):
    if binary.is_symlink() or sha(binary) != expected_digest:
        raise ValueError("unexpected generated executable digest")
    expected = read(REFERENCE)
    ids = []
    results = {}
    with tempfile.TemporaryDirectory(prefix="odd-candidate-owner-", dir="/private/tmp") as directory:
        root = Path(directory)
        for position, name in enumerate(SCOPED_CASES):
            work = root / str(position)
            work.mkdir(mode=0o700)
            result, dynamic_ids = observe(binary, work, *CASES[name])
            if result != expected["cases"][name]:
                raise ValueError(f"generated owner handoff differs for {name}")
            results[name] = result
            ids.extend(dynamic_ids)
        if len(ids) != 4 or len(set(ids)) != len(ids):
            raise ValueError("generated submit IDs are missing or reused")
        discovery = root / "discovery"
        discovery.mkdir(mode=0o700)
        endpoint = owner_discovery(binary, discovery)
        relays = root / "relays"
        relays.mkdir(mode=0o700)
        relay_order = owner_discovery(binary, relays, canonical_relays=True)
        denials = {}
        for name, change, diagnostic, extra in (
            ("untrusted", untrust, b"marmish: invalid input: untrusted_remote\n", ()),
            ("shared-config", lambda path: path.chmod(0o644),
             b"marmish: identity: unsafe_profile_file\n", ()),
            ("linked-config", symlink, b"marmish: identity: unsafe_profile_file\n", ()),
            ("hardlinked-config", hardlink, b"marmish: identity: unsafe_profile_file\n", ()),
            ("shared-profile", lambda path: path.parent.chmod(0o755),
             b"marmish: identity: unsafe_profile_path\n", ()),
            ("oversized-config", lambda path: path.write_bytes(b" " * 1_048_577),
             b"marmish: invalid input: config_size\n", ()),
            ("invalid-onion", invalid_onion, b"marmish: invalid input: destination_policy_mismatch\n", ()),
            ("route-mismatch", lambda _: None, b"marmish: invalid input: destination_policy_mismatch\n",
             ("--tor", "auto")),
            ("duplicate-config", duplicate_member, b"marmish: invalid input: config_schema\n", ()),
            ("relay-mismatch", lambda _: None, b"marmish: invalid input: destination_policy_mismatch\n",
             ("--relay", "wss://localhost:31338/")),
        ):
            work = root / name
            work.mkdir(mode=0o700)
            denials[name] = reject(binary, work, (1, b"", diagnostic), change, extra)
        for name, change, expected, options in (
            ("untrusted-discovered", untrust, (1, b"", b"marmish: invalid input: untrusted_remote\n"),
             {"discover": True}),
            ("oversized-stdin", lambda _: None, (255, b"", b"marmish: command_bytes_limit\n"),
             {"input_bytes": b"x" * 16385}),
        ):
            work = root / name
            work.mkdir(mode=0o700)
            denials[name] = reject(binary, work, expected, change, **options)
    return {
        "format": "marmish-basic-command-generated-owner-handoff-v1",
        "generatedClientSha256": expected_digest,
        "upstreamReceiptSha256": sha(REFERENCE),
        "probeSha256": sha(Path(__file__)),
        "workspaceRemoved": not root.exists(),
        "cases": results,
        "discoveredOwner": endpoint,
        "canonicalRelaySet": relay_order,
        "rejectedBeforeOwner": denials,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("generated_client", type=Path)
    parser.add_argument("expected_sha256")
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    result = verify(args.generated_client, args.expected_sha256)
    with args.receipt.open("x") as output:
        output.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
