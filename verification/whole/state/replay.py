"""Write a disposable bridge v1 state capsule or read it with a selected native pair."""
import argparse
import hashlib
import json
from pathlib import Path
import resource
import shutil
import subprocess
import tempfile

FIXTURE = Path(__file__).with_name("v1.json")
ID = "1234567890abcdef1234567890abcdef"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def files(root):
    return {str(p.relative_to(root)): digest(p) for p in sorted(root.rglob("*")) if p.is_file()}


def session(bridge, worker, root):
    cwd = root / "cwd"
    cwd.mkdir(mode=0o700, exist_ok=True)
    init = dict(op="init", version=1, state_root=str(root / "state"), cwd=str(cwd),
                worker_executable=str(worker), server="1"*64, allowed_principals=["2"*64])
    cases = [init, {"op":"maintain","limit":0}, {"op":"maintain","limit":1}, {"op":"cleanup_ready"}]
    data = b"".join((json.dumps(dict(req, id=ID), separators=(",", ":"))+"\n").encode() for req in cases)
    child = subprocess.run([bridge], input=data, capture_output=True, cwd=cwd, timeout=15,
                           env={"PATH":"/usr/bin:/bin","LANG":"C"}, umask=0o077,
                           preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_NOFILE, (256, 256)))
    return {"exit": child.returncode, "stdout": child.stdout.decode("ascii"), "stderr": child.stderr.decode("ascii")}


def replay(args):
    bridge, worker = args.bridge.resolve(strict=True), args.worker.resolve(strict=True)
    expected = json.loads(FIXTURE.read_text())
    capsule = args.capsule.resolve()
    if args.mode == "capture":
        reference = json.loads(args.writer_receipt.read_bytes())["builds"]["default"]["sha256"]
        if digest(bridge) != reference["marmish-engine-bridge"] or digest(worker) != reference["marmish-engine-worker"]:
            raise ValueError("state writer binaries differ from selected original receipt")
        capsule.mkdir(mode=0o700, parents=True, exist_ok=False)
        if session(bridge, worker, capsule) != expected:
            raise ValueError("original state writer differed from the frozen replies")
        (capsule / "manifest.json").write_text(json.dumps(files(capsule / "state"), sort_keys=True))
    if args.mode == "read" and digest(capsule / "manifest.json") != json.loads(args.state_receipt.read_bytes())["privateManifestSha256"]:
        raise ValueError("state capsule differs from selected original receipt")
    manifest = json.loads((capsule / "manifest.json").read_text())
    if files(capsule / "state") != manifest:
        raise ValueError("private input state differs from its captured manifest")
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory).resolve()
        shutil.copytree(capsule / "state", root / "state")
        observed = session(bridge, worker, root)
        if observed != expected or digest(root / "state/engine.key") != manifest["engine.key"]:
            raise ValueError("reader failed to reopen original state with identical replies and key")
        # Corrupt the disposable copy only. A reader that discards state would falsely succeed.
        (root / "state/engine.sqlite3").write_bytes(b"not a database")
        corrupt = session(bridge, worker, root)
        if corrupt["exit"] != 0 or corrupt["stderr"] or corrupt["stdout"] != json.loads(
                FIXTURE.with_name("corrupt.json").read_text())["stdout"]:
            raise ValueError(f"corrupt-state control differs: {corrupt!r}")
    print(json.dumps({"schema":1,"bridgeSha256":digest(bridge),"workerSha256":digest(worker),
                      "privateManifestSha256":digest(capsule / "manifest.json"),
                      "fixtureSha256":digest(FIXTURE),"corruptFixtureSha256":digest(FIXTURE.with_name("corrupt.json")),
                      "keyRetained":True,"reopened":True,"corruptStateRejected":True}, indent=2))


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["capture","read"])
    for name in ["bridge","worker","capsule"]:
        p.add_argument(name,type=Path)
    p.add_argument("--writer-receipt", type=Path, default=FIXTURE.parents[1] / "bridge/aarch64-apple-darwin.json")
    p.add_argument("--state-receipt", type=Path, default=FIXTURE.with_name("aarch64-apple-darwin.json"))
    replay(p.parse_args())
