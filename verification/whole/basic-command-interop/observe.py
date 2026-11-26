"""Probe the pinned upstream control using a test-owned onion relay."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import time

from corpus import SHORT_CASES, expected_receipt, finite_case
from isolate import Workspace, SYSTEM_HOME
from qualify import Output, detached_handle, finite_goodput


UPSTREAM = "af54339d250290490d62d4475a77539b7e739071"
BUILD = Path(__file__).with_name("reference-build-macos-arm64.json")
BRIDGE = Path(__file__).resolve().parents[1] / "bridge" / "aarch64-apple-darwin.json"
RELAY = re.compile(r"ws://[a-z2-7]{56}\.onion/relay")


def file_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_verified(source, target, expected):
    shutil.copyfile(source, target)
    target.chmod(0o500)
    if file_digest(target) != expected:
        raise ValueError("artifact digest differs")


def until(check, timeout, stage):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.25)
    raise TimeoutError(stage)


def account_options(instance, role, relay):
    return ["--state-dir", instance.root / "state", "--identity", role, "--key-storage", "file", "--tor", "auto", "--relay", relay]


def json_command(instance, args, setup_receipt=None):
    result = instance.run(args, timeout=30)
    if setup_receipt is not None:
        setup_receipt.update(Output(result.returncode, result.stdout, result.stderr).receipt())
    if result.returncode or result.stderr:
        raise ValueError("unexpected setup transcript")
    return json.loads(result.stdout)


def verify_profile(instance, role):
    profile = instance.root / "state" / role
    manifest = json.loads((profile / "profile.json").read_bytes())
    keys = profile / "keys"
    if (not keys.is_dir() or keys.is_symlink() or keys.stat().st_mode & 0o777 != 0o700
            or manifest["backend"] != "files" or manifest["phase"] != "ready"):
        raise ValueError("private file credential store not verified")
    children = list(keys.iterdir())
    if len(children) != 2 or any(not child.is_file() or child.is_symlink()
                                 or child.stat().st_mode & 0o777 != 0o600
                                 for child in children):
        raise ValueError("unexpected credential files")
    return manifest["public_key"]


def host_preferences_state():
    paths = [SYSTEM_HOME / "Library/Preferences/com.apple.security.plist",
             SYSTEM_HOME / "Library/Keychains", SYSTEM_HOME / ".marmish",
             SYSTEM_HOME / ".local/share/marmish-native", SYSTEM_HOME / ".config/marmish"]
    result = {}
    for path in paths:
        entries = [path, *sorted(path.iterdir())] if path.is_dir() else [path]
        for entry in entries:
            try:
                value = entry.lstat()
                result[entry] = (value.st_mode, value.st_ino, value.st_size, value.st_mtime_ns)
            except FileNotFoundError:
                result[entry] = None
    return result


def observe(binaries, deaddrop, deaddrop_digest, installed_fixture, fixture_digest, interpreter,
            generated_dir=None, generated_role=None):
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        raise ValueError("the reference preflight requires macOS ARM64")
    interpreter = interpreter.resolve(strict=True)
    if not interpreter.is_file() or not str(interpreter).isascii() or any(
            character.isspace() for character in str(interpreter)):
        raise ValueError("short fixture interpreter must have a simple absolute path")
    build = json.loads(BUILD.read_bytes())
    if (fixture_digest != build["fixture"]["sha256"]
            or deaddrop_digest != build["relay"]["sha256"]
            or file_digest(interpreter) != build["short_interpreter_sha256"]
            or file_digest(Path(__file__).with_name("corpus.py")) != build["corpus_source_sha256"]):
        raise ValueError("reference artifact does not match pinned build record")
    record = json.loads(BRIDGE.read_bytes())
    if record["commit"] != UPSTREAM:
        raise ValueError("wrong upstream provenance receipt")
    pins = record["builds"]["default"]["sha256"]
    observation = {"schema": 1, "upstream": UPSTREAM, "platform": "macos-arm64",
                   "reference_binary_sha256": pins, "relay_sha256": deaddrop_digest,
                   "fixture_sha256": fixture_digest, "short_interpreter_sha256": file_digest(interpreter),
                   "build_record_sha256": file_digest(BUILD), "stage": "sandbox",
                   "reference_complete": False, "attempts": []}
    artifact_dirs = dict.fromkeys(("client", "server"), binaries)
    artifact_pins = dict.fromkeys(("client", "server"), pins)
    if generated_dir is not None:
        candidate_record = generated_dir / "receipt.json"
        preflight = json.loads(Path(__file__).with_name("generated-preflight-aarch64-apple-darwin.json").read_bytes())
        qualified = json.loads(Path(__file__).with_name("generated-build-aarch64-apple-darwin.json").read_bytes())
        if file_digest(candidate_record) != preflight["buildReceiptSha256"]:
            raise ValueError("generated build receipt does not match the committed observation")
        candidate = json.loads(candidate_record.read_bytes())
        if candidate["artifacts"] != qualified["artifacts"]:
            raise ValueError("generated artifact digests do not match the committed build")
        if candidate["referenceTargetRevision"] != UPSTREAM or candidate["platform"] != "aarch64-apple-darwin":
            raise ValueError("generated candidate provenance mismatch")
        artifact_dirs[generated_role] = generated_dir
        artifact_pins[generated_role] = {name: record["sha256"]
                                         for name, record in candidate["artifacts"].items()}
        if not {"marmish", "marmishd"} <= artifact_pins[generated_role].keys():
            raise ValueError("generated candidate lacks a CLI or daemon artifact")
        if artifact_pins[generated_role].keys() - pins.keys():
            raise ValueError("generated candidate declares unexpected executable names")
        observation.update(generated_role=generated_role,
                           generated_receipt_sha256=file_digest(candidate_record),
                           generated_binary_sha256=artifact_pins[generated_role],
                           missing_generated_artifacts=sorted(pins.keys() - artifact_pins[generated_role].keys()))
    observation["setups"] = {}
    host_before = host_preferences_state()
    workspace = Workspace()
    try:
        with workspace:
            peers = {role: workspace.instance(role) for role in ("client", "server")}
            service = workspace.instance("relay")
            observation["sandbox_probes"] = True
            for role, instance in peers.items():
                for name, checksum in artifact_pins[role].items():
                    copy_verified(artifact_dirs[role] / name, instance.root / name, checksum)
            relay_bin = service.root / "deaddrop"
            copy_verified(deaddrop, relay_bin, deaddrop_digest)
            observation["stage"] = "relay-startup"
            with tempfile.TemporaryFile(dir=service.root) as transcript:
                relay = service.start([relay_bin, "relay", "--data-dir", service.root / "data"],
                                      stdin=subprocess.DEVNULL, stdout=transcript, stderr=subprocess.DEVNULL)

                def relay_url():
                    if relay.poll() is not None:
                        raise ValueError("owned relay stopped during startup")
                    first = os.pread(transcript.fileno(), 1024, 0)
                    if not first.endswith(b"\n"):
                        return None
                    address = json.loads(first)["relay_url"]
                    if not RELAY.fullmatch(address):
                        raise ValueError("unexpected owned relay endpoint")
                    return address

                address = until(relay_url, 150, "relay-startup")
                observation["owned_onion_constructed"] = True
                observation["stage"] = "offline-setup"
                accounts = {}
                account_keys = {}
                for role, instance in peers.items():
                    cli = instance.root / "marmish"
                    options = account_options(instance, role, address)
                    setup_receipt = {}
                    observation["setups"][role] = setup_receipt
                    setup = json_command(instance, [cli, *options, "setup", "--role", role, "--no-grants"],
                                         setup_receipt)
                    if (setup["backend"] != "file" or not setup["identity_created"]
                            or setup["identity"] != role or setup["inbound_enabled"]):
                        raise ValueError("unexpected disposable setup response")
                    if not re.fullmatch(r"npub1[02-9ac-hj-np-z]{58}", setup["npub"]):
                        raise ValueError("unexpected public identifier format")
                    account_keys[role] = verify_profile(instance, role)
                    accounts[role] = setup["npub"]
                if len(set(accounts.values())) != 2:
                    raise ValueError("disposable identities not distinct")
                observation["isolated_file_credentials"] = True
                observation["distinct_identities"] = True

                for role, instance in peers.items():
                    other = "server" if role == "client" else "client"
                    flag = "--trust" if role == "client" else "--grant"
                    options = account_options(instance, role, address)
                    update = json_command(instance, [instance.root / "marmish", *options,
                                                      "setup", "--update", flag, accounts[other]])
                    if (update["backend"] != "file" or update["identity_created"]
                            or update["npub"] != accounts[role]
                            or update["inbound_enabled"] != (role == "server")):
                        raise ValueError("unexpected setup authorization response")
                    config = json.loads((instance.root / "state" / role / "config.json").read_bytes())
                    expected_clients = [account_keys[other]] if role == "server" else []
                    expected_remotes = [account_keys[other]] if role == "client" else []
                    if (config["execution_clients"] != expected_clients
                            or config["trusted_remotes"] != expected_remotes
                            or config["relays"] != [address] or config["tor"] != "auto"):
                        raise ValueError("refusing unexpected peer authorization or relay scope")
                observation["explicit_peer_authorization_verified"] = True
                if observation.get("missing_generated_artifacts"):
                    observation["stage"] = "missing-generated-artifacts"
                    raise ValueError("generated helper roles unavailable")
                server = peers["server"]
                policy_path = server.root / "state" / "server" / "config.json"
                fixtures = (server.root / "short", server.root / "finite")
                short_source = Path(__file__).with_name("corpus.py")
                script = short_source.read_bytes().split(b"\n", 1)[1]
                fixtures[0].write_bytes(b"#!" + os.fsencode(interpreter) + b"\n" + script)
                fixtures[0].chmod(0o500)
                observation["short_fixture_sha256"] = file_digest(fixtures[0])
                copy_verified(installed_fixture, fixtures[1], fixture_digest)
                (server.root / "receipts").mkdir(mode=0o700)
                policy = json.loads(policy_path.read_bytes())
                policy["command_policy"]["defaults"] = {"allow_exec": list(map(str, fixtures))}
                policy["cwd"] = str(server.root)
                policy_path.write_text(json.dumps(policy))
                observation["stage"] = "owner-activation"
                with ExitStack() as logs:
                    owner_logs = []
                    try:
                        for role in ("server", "client"):
                            instance = peers[role]
                            options = account_options(instance, role, address)
                            # The upstream engine reopens every inherited vnode by its named path.
                            log_path = instance.root / "owner.stderr"
                            fd = os.open(log_path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
                            owner_log = logs.enter_context(os.fdopen(fd, "w+b"))
                            owner_logs.append(owner_log)
                            child = instance.start(
                                [instance.root / "marmishd", *options, "--foreground", "daemon"],
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=owner_log)

                            def ready():
                                if child.poll() is not None:
                                    raise ValueError(f"{role} owner stopped during activation")
                                status = instance.run([instance.root / "marmish", *options, "status"], timeout=5)
                                if status.returncode:
                                    return False
                                return (json.loads(status.stdout).get("result") or {}).get("state") == "active"

                            until(ready, 170, "owner-activation")
                            observation[f"{role}_active"] = True
                    except (ValueError, TimeoutError, subprocess.TimeoutExpired):
                        transcript_bytes = b"\n".join(os.pread(log.fileno(), 65536, 0) for log in owner_logs)
                        codes = [line for line in (b"marmish: marmot: key_package_publish",
                                                   b"marmish: transport: relay_connect")
                                 if line in transcript_bytes.splitlines()]
                        observation["terminal_diagnostics"] = [item.decode() for item in codes]
                        raise
                    observation["both_upstream_owners_active"] = True
                    for instance in peers.values():
                        if len([p for p in instance.tmp.glob("mr2-*/s") if p.is_socket()]) != 1:
                            raise ValueError("private daemon socket not verified")
                    observation["stage"] = "command-corpus"
                    client = peers["client"]
                    base = [client.root / "marmish", *account_options(client, "client", address),
                            "--no-autostart", "--wait-seconds", "300"]
                    reference_commands(observation["attempts"], client, base, accounts["server"],
                                       account_keys["server"], fixtures)
                    observation["stage"] = "complete"
                    observation["reference_complete"] = generated_role is None
    except (ValueError, TimeoutError, subprocess.TimeoutExpired, InterruptedError) as error:
        observation["failure_type"] = type(error).__name__
        outcome = "blocked"
    else:
        outcome = "reference-pass" if generated_role is None else "mixed-pass"
    observation["workspace_removed"] = hasattr(workspace, "root") and not workspace.root.exists()
    observation["host_preferences_metadata_unchanged"] = host_preferences_state() == host_before
    if not observation["workspace_removed"] or not observation["host_preferences_metadata_unchanged"]:
        observation["cleanup_verification"] = "blocked"
        observation["reference_complete"] = False
        outcome = "blocked"
    observation.setdefault("cleanup_verification", "passed")
    return dict(observation, outcome=outcome)



def reference_commands(attempts, client, base, server_npub, server_hex, fixtures):
    handles = set()
    expected_files = set()
    for repetition in range(4):
        cases = (*SHORT_CASES, finite_case(repetition))
        for case in cases:
            elapsed_start = time.monotonic_ns()
            attempt = {"case": case.name, "repetition": repetition,
                       "temperature": "cold" if repetition == 0 else "warm", "matched": False}
            attempts.append(attempt)
            finite = case.name == "finite"
            exe = fixtures[1] if finite else fixtures[0]
            nonce = hashlib.sha256(f"{case.name}:{repetition}".encode()).hexdigest().encode()
            args = case.args if finite else (case.name.encode(), nonce)
            command = [*base, "--stdin"]
            if finite:
                command.append("--detach")
            command.extend((server_npub, "--", exe, *args))
            result = client.run(command, case.stdin)
            output = Output(result.returncode, result.stdout, result.stderr)
            if finite:
                locator = detached_handle(output, server_hex)
                invocation = json.loads(locator)["invocation"]
                if invocation in handles:
                    raise ValueError("reused detached invocation")
                handles.add(invocation)
                attempt["handle_format_and_receiver_verified"] = True
                remaining = 330 - (time.monotonic_ns() - elapsed_start) / 1e9
                if remaining <= 0:
                    raise TimeoutError("finite-benchmark")
                waited = client.run([*base, "wait", "--handle", locator], timeout=remaining)
                output = Output(waited.returncode, waited.stdout, waited.stderr)
                nonce = args[0]
            elapsed = time.monotonic_ns() - elapsed_start
            expected = Output(case.status, case.stdout, case.stderr)
            attempt["result"] = output.receipt()
            attempt["elapsed_ns"] = elapsed
            output.compare(expected)
            receipt_path = exe.parent / "receipts" / f"{nonce.decode()}.receipt"
            expected_files.add(receipt_path.name)
            if not receipt_path.is_file():
                raise ValueError("missing server fixture receipt")
            receipt = json.loads(receipt_path.read_bytes())
            if receipt != expected_receipt(case, exe, args):
                raise ValueError("server fixture receipt mismatch")
            attempt.update(matched=True, server_receipt_verified=True)
            if finite:
                attempt["bytes_per_second"] = finite_goodput(output, expected, elapsed)
    actual_files = {path.name for path in (fixtures[0].parent / "receipts").iterdir()}
    if actual_files != expected_files:
        raise ValueError("extra or missing receiver fixture receipts")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream_bin_dir", type=Path)
    parser.add_argument("deaddrop", type=Path)
    parser.add_argument("deaddrop_sha256")
    parser.add_argument("installed_fixture", type=Path)
    parser.add_argument("fixture_sha256")
    parser.add_argument("interpreter", type=Path, help="real system Python, bypassing the xcrun launcher")
    parser.add_argument("--generated-dir", type=Path)
    parser.add_argument("--generated-role", choices=("client", "server"))
    args = parser.parse_args()
    if (args.generated_dir is None) != (args.generated_role is None):
        parser.error("provide both --generated-dir and --generated-role")
    receipt = observe(args.upstream_bin_dir, args.deaddrop, args.deaddrop_sha256,
                      args.installed_fixture, args.fixture_sha256, args.interpreter,
                      args.generated_dir, args.generated_role)
    print(json.dumps(receipt, indent=2, sort_keys=True))
    return 0 if receipt["outcome"] in {"reference-pass", "mixed-pass"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
