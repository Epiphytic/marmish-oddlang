#!/usr/bin/env python3
"""Build the Linux reference caller, then judge it from a separate macOS host."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from framework_pin import REVISION, verify_checkout
from replay import LOCK, PIN


def driver(root):
    verify_checkout(root)
    sys.path.insert(0, str(root / "examples/marmish/portable-oracle"))
    import run
    return run


def build(args):
    oracle = driver(args.framework.resolve())
    check_host = subprocess.check_output(["rustc", "+1.97.1", "-vV"], text=True)
    triple, = [s.split()[1] for s in check_host.splitlines() if s.startswith("host:")]
    if triple not in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"):
        raise ValueError("native Linux toolchain required")
    args.directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    command, receipt = oracle.build_original(args.original.resolve(), args.directory.resolve(),
                                             PIN, LOCK, args.profile, triple)
    oracle.write(args.directory / "build.json", {"framework": REVISION, "program": command[0], "origin": receipt})
    verify_checkout(args.framework)


def isolated_guest(args):
    ssh = ["/usr/bin/ssh", "-T", "-F", str(args.ssh_config.resolve()), "-o", "ForwardAgent=no",
           "-o", "ClearAllForwardings=yes", "-o", "ControlPath=none", "-o", "ProxyCommand=none",
           "-o", "ProxyJump=none"]
    config = subprocess.check_output([*ssh, "-G", args.guest], text=True)
    if not {"hostname 127.0.0.1", "forwardagent no"}.issubset(config.splitlines()):
        raise ValueError("expected a loopback-only guest SSH connection without agent forwarding")
    command = [*ssh, args.guest]

    def remote(*argv, check=True):
        return subprocess.run([*command, shlex.join(argv)], timeout=15, capture_output=True, check=check)

    mounts = remote("findmnt", "-rn", "-t", "virtiofs,9p,cifs,nfs,nfs4", check=False)
    if (mounts.returncode, mounts.stdout, mounts.stderr) != (1, b"", b""):
        raise ValueError("guest has shared mounts or mount enumeration failed")
    with tempfile.NamedTemporaryFile(prefix="oddlang-origin-boundary-") as sentinel:
        remote("test", "!", "-e", sentinel.name)
    return command, remote


def judge(args):
    oracle = driver(args.framework.resolve())
    command, remote = isolated_guest(args)
    receipt = json.loads(remote("cat", str(args.build)).stdout)
    provenance = receipt["origin"]
    caller = receipt["program"]
    if (receipt["framework"], provenance["revision"], provenance["cargoLockSha256"]) != (REVISION, PIN, LOCK):
        raise ValueError("unexpected guest provenance")
    identity = remote("sha256sum", "--", caller).stdout.decode().split()[0]
    if identity != provenance["callerSha256"]:
        raise ValueError("mismatched guest caller")
    freeze = json.loads(oracle.FREEZE.read_bytes())
    cases = oracle.sealed_cases(args.holdout, freeze)
    boundary = oracle.holdout_boundary(args.holdout)
    try:
        observed = oracle.observe([*boundary, *command, shlex.quote(caller)], cases, provenance["callerBinding"])
        if oracle.failures(cases, observed):
            raise ValueError("sealed reference behavior differs")
    except Exception:
        raise RuntimeError("isolated sealed replay failed") from None
    after = remote("sha256sum", "--", caller).stdout.decode().split()[0]
    if identity != after:
        raise ValueError("guest executable changed during replay")
    oracle.write(args.output, {"framework": REVISION, "origin": provenance,
        "frozenCapsuleSha256": freeze["holdout"]["fileSha256"], "sealedCases": len(cases), "passed": True,
        "boundary": {"guestHost": "127.0.0.1", "agentForwarding": False, "sharedMounts": [],
                     "hostSentinelAbsentInGuest": True, "hostClientCapsuleReadProbe": "EPERM"}})


def judge_data(args):
    oracle = driver(args.framework.resolve())
    import qualify_tagged as expected
    command, remote = isolated_guest(args)
    remote("test", "!", "-e", str(args.guest_receipt))
    freeze = json.loads(oracle.FREEZE.read_bytes())
    cases = [case for case in oracle.sealed_cases(args.holdout, freeze) if expected.comparable(case)]
    requests = [line for case in cases for line in expected.case_inputs(case)]
    wanted = [line for case in cases for line in expected.expected(case)]
    argv = [*args.command, "--external-judge", "--receipt", str(args.guest_receipt)]
    try:
        result = subprocess.run([*oracle.holdout_boundary(args.holdout), *command, shlex.join(argv)],
                                input=json.dumps(requests), text=True, capture_output=True, timeout=300, check=True)
        if json.loads(result.stdout) != {"rust": wanted, "typescript": wanted}:
            raise ValueError("unexpected observations")
    except Exception:
        raise RuntimeError("isolated generated target comparison failed") from None
    receipt_bytes = remote("cat", str(args.guest_receipt)).stdout
    receipt = json.loads(receipt_bytes)
    driver_pins = {
        "linux_boundary.py": "559e56472735f81ca0400e221c207b202a632b0358fd8d0e3695adeefe30e387",
        "qualify_tagged.py": "f2fd54fc8768215eb41d4052b8194f15699f4bc72a5266a9e07ebf7086b6a973",
        "tagged_adapter.mjs": "5be9dd353fabb41c58439c4b3dda71e1a1252e648c24b5a6d3d8eac737643c52",
        "tagged_adapter.rs": "8f8315f5975326950ec9ca9bcb08192e9b382f7951c642a0e4f3e000247cf2f1",
    }
    if receipt["driverSourceSha256"] != driver_pins:
        raise ValueError("unexpected guest qualifier or adapter")
    pinned = json.loads((expected.ROOT / "tagged-qualification.json").read_bytes())
    for field in ("approvedGraphSha256", "publicCaseIds", "comparedOperationResults", "mixedPeers",
                  "mutation"):
        if receipt[field] != pinned[field]:
            raise ValueError(f"public receipt differs at {field}")
    if receipt["upstreamRevision"] != PIN or receipt["originalCaller"]["cargoLockSha256"] != LOCK:
        raise ValueError("unexpected original revision or lock")
    if receipt["build"]["generatedSourceSha256"] != pinned["build"]["generatedSourceSha256"]:
        raise ValueError("public receipt has unexpected generated source")
    oracle.write(args.output, {"framework": REVISION, "upstream": PIN,
        "frozenCapsuleSha256": freeze["holdout"]["fileSha256"],
        "guestReceiptSha256": hashlib.sha256(receipt_bytes).hexdigest(),
        "generatedSourceSha256": receipt["build"]["generatedSourceSha256"],
        "sealedCasesPerRole": len(cases), "comparedOperationsPerRole": len(wanted),
        "passedRoles": ["rust", "typescript"], "boundary": {"guestHost": "127.0.0.1",
        "agentForwarding": False, "sharedMounts": [], "hostSentinelAbsentInGuest": True,
        "hostClientCapsuleReadProbe": "EPERM"}})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(required=True)
    guest = modes.add_parser("build")
    guest.add_argument("framework", type=Path)
    guest.add_argument("original", type=Path)
    guest.add_argument("profile", choices=("dev", "release"))
    guest.add_argument("directory", type=Path)
    guest.set_defaults(action=build)
    host = modes.add_parser("judge")
    host.add_argument("framework", type=Path)
    host.add_argument("ssh_config", type=Path)
    host.add_argument("guest")
    host.add_argument("build", type=Path)
    host.add_argument("holdout", type=Path)
    host.add_argument("output", type=Path)
    host.set_defaults(action=judge)
    data = modes.add_parser("judge-data")
    data.add_argument("framework", type=Path)
    data.add_argument("ssh_config", type=Path)
    data.add_argument("guest")
    data.add_argument("holdout", type=Path)
    data.add_argument("guest_receipt", type=Path)
    data.add_argument("output", type=Path)
    data.add_argument("command", nargs=argparse.REMAINDER)
    data.set_defaults(action=judge_data)
    args = parser.parse_args()
    args.action(args)
