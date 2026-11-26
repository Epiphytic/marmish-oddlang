#!/usr/bin/env python3
"""Build the Linux reference caller, then judge it from a separate macOS host."""
import argparse
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


def judge(args):
    oracle = driver(args.framework.resolve())
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
    args = parser.parse_args()
    args.action(args)
