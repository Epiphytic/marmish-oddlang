#!/usr/bin/env python3
"""Qualify the native foreground owner on a profile created by the pinned originals."""

import argparse
import ast
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import socket
import ssl
import stat
import subprocess
import sys
import tempfile
import time

from project import INPUTS, MANIFEST, REFERENCE

HERE = Path(__file__).resolve().parent
SUCCESS = {"exit": 0, "stdout": '{"ok":true,"result":{"state":"activating"},"error":null}\n', "stderr": ""}
QUIET = {"exit": 0, "stdout": "", "stderr": ""}
LOCKED = {"exit": 1, "stdout": "", "stderr": "marmish: identity: profile_in_use\n"}
MISSING = {"exit": 1, "stdout": "", "stderr": "marmish: local I/O failed\n"}


def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(actual, expected, label):
    if actual != expected:
        raise ValueError(f"{label}: expected {expected!r}, observed {actual!r}")


def observation(code, out, err):
    return {"exit": code, "stdout": out.decode(), "stderr": err.decode()}


def call(argv, env, cwd):
    result = subprocess.run(
        argv, env=env, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, umask=0o077, timeout=6,
    )
    return observation(result.returncode, result.stdout, result.stderr)


def hold(argv, env, root, runtime, profile, relay, action, *, terminate=signal.SIGTERM):
    child = subprocess.Popen(
        argv, env=env, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        stdin=subprocess.DEVNULL, umask=0o077,
    )
    peer = None
    try:
        deadline = time.monotonic() + 6
        endpoint = runtime / "s"
        while not endpoint.is_socket() or stat.S_IMODE(endpoint.stat().st_mode) != 0o600:
            if child.poll() is not None or time.monotonic() >= deadline:
                raise ValueError("foreground owner did not publish its private socket")
            time.sleep(0.01)
        check(stat.S_IMODE(runtime.stat().st_mode), 0o700, "runtime mode")
        binding = runtime / "binding.json"
        check(stat.S_IMODE(binding.stat().st_mode), 0o600, "binding mode")
        document = json.loads(binding.read_bytes())
        check(document, {"format": 2, "profile_id": profile.name_token, "root": str(profile.path)}, "runtime binding")
        peer, _ = relay.accept()
        peer.settimeout(5)
        first = bytearray()
        while len(first) < 3:
            data = peer.recv(3 - len(first))
            if not data:
                raise ValueError("relay connection closed before a TLS record")
            first.extend(data)
        check(bytes(first), b"\x16\x03\x01", "outbound TLS ClientHello record")
        result = action()
        result["tlsRecordPrefix"] = first.hex()
    finally:
        if child.poll() is None:
            child.send_signal(terminate)
        try:
            stdout, stderr = child.communicate(timeout=6)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()
            if peer is not None:
                peer.close()
    check(runtime.exists(), terminate == signal.SIGKILL, "owned runtime presence after termination")
    return result, observation(child.returncode, stdout, stderr)


class PreparedProfile:
    def __init__(self, profile):
        self.path = profile
        self.document = json.loads((profile / "profile.json").read_bytes())
        self.name_token = self.document["profile_id"]
        check(self.document["phase"], "ready", "original provisioned profile")
        check(self.document["backend"], "files", "original provisioned backend")

    def credential(self, prefix, field):
        return self.path / "keys" / f"{prefix}-{self.document[field]}"


def rejected_certificate(executable, common, root, runtime, env, server):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(15)
        url = f"wss://127.0.0.1:{listener.getsockname()[1]}"
        child = subprocess.Popen(
            [executable, "--foreground", *common, "--key-storage", "file", "--tor", "off", "--relay", url],
            env=env, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL, umask=0o077,
        )
        try:
            conn, _ = listener.accept()
            with conn:
                conn.settimeout(8)
                try:
                    with server.wrap_socket(conn, server_side=True):
                        raise ValueError("foreground owner accepted the untrusted certificate")
                except ssl.SSLError as error:
                    if error.reason not in {"SSLV3_ALERT_CERTIFICATE_UNKNOWN", "TLSV1_ALERT_UNKNOWN_CA"}:
                        raise
            # Close the listener so the reference cannot wait inside a later retry.
            listener.close()
            out, err = child.communicate(timeout=25)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()
    report = observation(child.returncode, out, err)
    check((report["exit"], report["stderr"]), (1, "marmish: transport: relay_connect\n"), "TLS trust failure")
    check(runtime.exists(), False, "failed TLS runtime cleanup")
    return report


@contextmanager
def held_relay():
    with socket.socket() as server:
        server.bind(("127.0.0.1", 0))
        server.listen()
        server.settimeout(5)
        yield server


def relay_options(listener, *, scheme="wss"):
    return ["--key-storage", "file", "--tor", "off",
            "--relay", f"{scheme}://127.0.0.1:{listener.getsockname()[1]}"]


def observe(original, generated, root, *, trust_failure=False):
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "HOME": str(root / "home"), "TMPDIR": str(root / "runtime")}
    for name in ("home", "runtime"):
        (root / name).mkdir(mode=0o700)
    client = original / "marmish"
    common = ["--state-dir", root / "state", "--identity", "station"]
    state = root / "state" / "station"

    def start(binary, relay):
        return [binary, "--foreground", *common, *relay_options(relay)]

    with held_relay() as relay:
        init = call([client, *common, *relay_options(relay), "init"], env, root)
    check((init["exit"], init["stderr"]), (0, ""), "pinned original profile provisioning")
    profile = PreparedProfile(state)
    runtime = root / "runtime" / f"mr2-{os.geteuid()}-{profile.name_token}"
    query = [client, *common, "status"]

    def inspect(competitor):
        status = call(query, env, root)
        check(status, SUCCESS, "original client auto-discovery status")
        denied = call(competitor, env, root)
        check(denied, LOCKED, "cross-implementation owner lock")
        return {"status": status, "competingOwner": denied}

    original_bin, generated_bin = original / "marmishd", generated / "marmishd"
    # A contender uses its own port, so only the selected owner can supply the observed handshake.
    with held_relay() as relay, held_relay() as competitor:
        original_result, original_stop = hold(
            start(original_bin, relay), env, root, runtime, profile, relay,
            lambda: inspect(start(generated_bin, competitor)),
        )
    check(original_stop["exit"], 0, "original signal stop")
    for flag in ("engine_stop_complete", "mdk_stop_complete", "runtime_cleanup_complete"):
        check(json.loads(original_stop["stdout"])[flag], True, flag)
    check(call(query, env, root), MISSING, "original post-stop discovery")

    prior_manifest = (state / "profile.json").read_bytes()
    with held_relay() as relay, held_relay() as competitor:
        generated_result, generated_stop = hold(
            start(generated_bin, relay), env, root, runtime, profile, relay,
            lambda: inspect(start(original_bin, competitor)),
        )
    check(generated_result, original_result, "foreground takeover status and exclusivity")
    check(generated_stop, QUIET, "generated local cleanup")
    check(call(query, env, root), MISSING, "generated post-stop discovery")
    expected_held = {"status": SUCCESS, "tlsRecordPrefix": "160301"}
    with held_relay() as relay:
        reattach, reattach_stop = hold(
            [*start(generated_bin, relay), "daemon"], env, root, runtime, profile, relay,
            lambda: {"status": call(query, env, root)},
        )
    check(reattach, expected_held, "explicit daemon subcommand reattach")
    check(reattach_stop, QUIET, "reattach cleanup")
    with held_relay() as relay:
        _, killed = hold(
            start(generated_bin, relay), env, root, runtime, profile, relay,
            lambda: {"status": call(query, env, root)}, terminate=signal.SIGKILL,
        )
    check(killed["exit"], -signal.SIGKILL, "abrupt termination")
    check((runtime / "binding.json").is_file() and (runtime / "s").is_socket(), True,
          "prior runtime retained after SIGKILL")
    with held_relay() as relay:
        recovered, recovered_stop = hold(
            start(generated_bin, relay), env, root, runtime, profile, relay,
            lambda: {"status": call(query, env, root)},
        )
    check(recovered, expected_held, "stale runtime recovery")
    check(recovered_stop, QUIET, "recovered owner cleanup")
    check((state / "profile.json").read_bytes(), prior_manifest, "no profile rewrite")

    controls = {}
    with held_relay() as relay:
        plaintext = relay_options(relay, scheme="ws")
        for executable in (original_bin, generated_bin):
            outcome = call([executable, "--foreground", *common, *plaintext], env, root)
            check(outcome, {"exit": 1, "stdout": "", "stderr": "marmish: invalid input: plaintext_relay\n"},
                  "plaintext relay rejection before owner publication")
        controls["plaintext-relay"] = outcome
        check(runtime.exists(), False, "plaintext rejection left runtime unpublished")
        startup = start(generated_bin, relay)
        signing = profile.credential("nostr", "nostr_ref")
        original_secret = signing.read_bytes()
        replacement = (1).to_bytes(32, "big")
        try:
            signing.write_bytes(replacement if original_secret != replacement else (2).to_bytes(32, "big"))
            controls["identity-mismatch"] = call(startup, env, root)
        finally:
            signing.write_bytes(original_secret)
        data = profile.credential("sqlcipher", "sqlcipher_ref")
        try:
            data.chmod(0o644)
            controls["unsafe-credential"] = call(startup, env, root)
        finally:
            data.chmod(0o600)
        for label, category in (("identity-mismatch", "identity_mismatch"), ("unsafe-credential", "unsafe_profile_file")):
            check(controls[label], {
                "exit": 1, "stdout": "", "stderr": f"marmish: identity: {category}\n",
            }, label)
        check(runtime.exists(), False, "failed preconditions did not publish runtime")
        runtime.mkdir(mode=0o700)
        marker = runtime / "unrelated"
        try:
            marker.write_bytes(b"retain")
            controls["runtime-collision"] = call(startup, env, root)
            check(controls["runtime-collision"], {
                "exit": 1, "stdout": "", "stderr": "marmish: identity: runtime_collision\n",
            }, "runtime collision")
            check(marker.read_bytes(), b"retain", "collision left existing runtime alone")
        finally:
            marker.unlink()
            runtime.rmdir()

    trust = {}
    if trust_failure:
        sys.path.insert(0, str(HERE.parent / "relay"))
        from owner_tls import certificate
        with tempfile.TemporaryDirectory(dir=root, prefix="keys-") as directory:
            server = certificate(Path(directory))
            trust["original"] = rejected_certificate(original / "marmishd", common, root, runtime, env, server)
            trust["generated"] = rejected_certificate(generated / "marmishd", common, root, runtime, env, server)
        check(trust["generated"]["stdout"], "", "no provider stop report")
        for flag in ("engine_stop_complete", "mdk_stop_complete", "runtime_cleanup_complete"):
            check(json.loads(trust["original"]["stdout"])[flag], True, flag)

    return {
        "original": original_result, "generated": generated_result, "originalShutdown": original_stop,
        "generatedShutdown": generated_stop, "reattachedStatus": reattach, "reattachedShutdown": reattach_stop,
        "staleRuntimeRecovered": recovered, "recoveredOwnerShutdown": recovered_stop,
        "postStopDiscovery": MISSING, "profileUnmodified": True, "candidateControls": controls,
        "privateCertificateRejected": trust,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=lambda value: Path(value).resolve())
    parser.add_argument("package", type=lambda value: Path(value).resolve())
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    parser.add_argument("--build-receipt", type=Path)
    args = parser.parse_args()
    revision = subprocess.check_output(["git", "-C", HERE, "rev-parse", "HEAD"], text=True).strip()
    if subprocess.check_output(["git", "-C", HERE, "status", "--porcelain", "--untracked-files=all"]):
        raise ValueError("qualification requires a clean companion checkout")
    targets = {("Darwin", "arm64"): "aarch64-apple-darwin", ("Linux", "aarch64"): "aarch64-unknown-linux-gnu"}
    target = targets[(platform.system(), platform.machine())]
    source = args.build_receipt
    if source is None:
        if target != "aarch64-apple-darwin":
            parser.error("--build-receipt is required outside macOS ARM64")
        source = HERE.parent / "bridge/aarch64-apple-darwin.json"
    source = source.resolve()
    provenance = json.loads(source.read_bytes())
    check(provenance["commit"], REFERENCE, "original release pin")
    originals = provenance["builds"]["default"]["sha256"]
    for name, digest in originals.items():
        check(file_hash(args.original / name), digest, f"original {name}")
    for filename, pin in (("Cargo.lock", "foreground-Cargo.lock"), ("package.lock", "package.lock")):
        check(file_hash(args.package / filename), file_hash(INPUTS / pin), filename)
    with tempfile.TemporaryDirectory(prefix="gfo-", dir="/tmp") as path:
        binary = args.package / "target/release"
        report = observe(args.original, binary, Path(path).resolve(), trust_failure=True)
    parser_output = subprocess.check_output(
        [sys.executable, HERE.parent / "cli/replay.py", "verify", binary], text=True, timeout=60,
    )
    report.update(
        platform=target, referenceRevision=REFERENCE, companionRevision=revision,
        frameworkRevision=args.framework_revision, originalBinaries=originals,
        originalBuildReceiptSha256=file_hash(source), candidateSha256=file_hash(binary / "marmishd"),
        packageSha256=file_hash(MANIFEST), packageLockSha256=file_hash(INPUTS / "package.lock"),
        parserObservations=ast.literal_eval(parser_output),
        unqualified=[
            "fresh generated profile initialization",
            "admin/provider overrides, background launch and installed executable lifecycle",
            "SQLCipher database ownership, WebSocket/MDK readiness, active/degraded status and command execution",
            "provider shutdown reports and recovery of corrupt or conflicting runtime directories",
            "authenticated TLS completion and production-length relay deadline",

        ],
    )
    with args.receipt.open("x") as output:
        output.write(json.dumps(report, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
