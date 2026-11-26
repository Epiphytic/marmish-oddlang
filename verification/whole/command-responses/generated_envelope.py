"""Compare a freshly generated response codec with the pinned mainline observations."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from envelope import cases
from verify import HERE, MODEL as SHAPE, ORIGINAL, REQUESTS, compact, observe

MODEL = HERE.parents[2] / "oddlang/portable/command-responses-v1"
REFERENCE = HERE / "reference-envelope-aarch64-apple-darwin.json"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def workspace_evidence(root, expected_lock, source):
    lock = (root/"workspace.lock").read_bytes()
    if lock != expected_lock.read_bytes():
        raise ValueError("workspace differs from the pinned selection")
    member = root/"marmish-native"
    package = (member/"package.lock").read_bytes()
    if sha(package) != json.loads(lock)["members"]["marmish-native"]["standalonePackageLockSha256"]:
        raise ValueError("native member differs from the pinned workspace")
    matches = [name for name, digest in json.loads(package)["generatedFiles"].items()
               if digest == sha(source)]
    if len(matches) != 1 or (member/matches[0]).read_bytes() != source:
        raise ValueError("expected the verified response source once in the native member")
    return {"workspaceLockSha256": sha(lock), "nativePackageLockSha256": sha(package),
            "sourcePath": matches[0]}


def verify(compiler, workspace=None, expected_lock=None):
    reference = json.loads(REFERENCE.read_bytes())
    if reference["referenceRevision"] != ORIGINAL:
        raise ValueError("unexpected mainline revision")
    rows = list(cases())
    names = [name for name, _, _ in rows]
    if len(set(names)) != len(rows) or set(names) != reference["cases"].keys():
        raise ValueError("case inventory differs from the frozen reference")
    inputs = []
    expected = []
    for name, raw, _ in rows:
        case = reference["cases"][name]
        if len(raw) != case["wireBytes"] or sha(raw) != case["wireSha256"]:
            raise ValueError(f"input bytes differ from the frozen reference for {name}")
        inputs.append(compact([json.loads(raw)["op"], raw.hex()]) + b"\n")
        expected.append(raw.hex() if case["accepted"] else None)

    with tempfile.TemporaryDirectory(prefix="generated-response-envelopes-") as tmp:
        root = Path(tmp)
        projection = root / "projection"
        subprocess.run([compiler, "build", "wire-protocol", "rust", SHAPE/"graph.json",
                        SHAPE/"rust.json", MODEL/"graph.json", MODEL/"rust.json",
                        projection], check=True)
        source = (projection/"wire.rs").read_bytes()
        output = observe(root/"candidate", source, b"".join(inputs), None, REQUESTS/"generated.rs")
        for name, want, line in zip(names, expected, output.splitlines(), strict=True):
            if json.loads(line) != want:
                raise ValueError(f"generated response differs from the frozen reference for {name}")
        receipt = {"referenceRevision": ORIGINAL, "referenceReceiptSha256": sha(REFERENCE.read_bytes()),
                "cases": len(rows), "accepted": sum(x is not None for x in expected),
                "projectionLockSha256": sha((projection/"projection.lock").read_bytes()),
                "observationsSha256": sha(output), "compilerSha256": sha(compiler.read_bytes())}
        if workspace is not None:
            receipt["workspace"] = workspace_evidence(workspace, expected_lock, source)
        return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("--workspace", type=Path, help="generated workspace to check")
    parser.add_argument("--workspace-lock", type=Path, help="independent pinned workspace.lock")
    args = parser.parse_args()
    if (args.workspace is None) != (args.workspace_lock is None):
        parser.error("--workspace and --workspace-lock must be supplied together")
    print(json.dumps(verify(args.compiler.resolve(), args.workspace, args.workspace_lock),
                     indent=2, sort_keys=True))
