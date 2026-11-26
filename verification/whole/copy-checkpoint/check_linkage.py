"""Check both sides of the released Checkpoint-to-Manifest reference."""
import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

PORTABLE = Path(__file__).resolve().parents[3] / "oddlang/portable"
FRAMEWORK = "70821c898cabe15a5d0483dc9425e51aae883932"
SOURCE = "stream-package-v5.json"
GRAPH = "copy-checkpoint-v1/graph.json"
OVERLAY = "copy-checkpoint-v1/rust.json"
MANIFEST = "urn:marmish:copy:manifest"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(cli, root):
    manifest = json.loads((PORTABLE / SOURCE).read_bytes())
    pins = [manifest["cargoLock"]]
    for component in manifest["components"]:
        pins.extend((component["graph"], component["overlay"]))
        if "wireShape" in component:
            pins.extend(component["wireShape"].values())
    for pin in pins:
        data = (PORTABLE / pin["path"]).read_bytes()
        assert sha(data) == pin["sha256"]
        destination = root / pin["path"]
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    sealed = json.loads((PORTABLE / "workspace-v5/workspace.lock").read_bytes())["members"]["marmish-stream"]
    assert sha((PORTABLE / SOURCE).read_bytes()) == sealed["inputSha256"]
    component = next(c for c in manifest["components"] if c["graph"]["path"] == GRAPH)
    graph = json.loads((root / GRAPH).read_bytes())
    overlay = json.loads((root / OVERLAY).read_bytes())
    results = {}

    def build(label, g=graph, b=overlay, expected=None):
        release = deepcopy(manifest)
        target = release["components"][manifest["components"].index(component)]
        for key, payload in (("graph", g), ("overlay", b)):
            path = target[key]["path"]
            data = (PORTABLE / path).read_bytes() if (payload is graph or payload is overlay) else json.dumps(payload).encode()
            (root / path).write_bytes(data)
            target[key]["sha256"] = sha(data)
        data = (PORTABLE / SOURCE).read_bytes() if label == "released" else json.dumps(release).encode()
        (root / SOURCE).write_bytes(data)
        out = root / label
        result = subprocess.run([cli, "build", "native-package", "rust", root / SOURCE, out],
                                capture_output=True, text=True, cwd=root)
        if expected is None:
            assert result.returncode == 0, result.stderr
            results[label] = sha((out / "package.lock").read_bytes())
        else:
            assert result.returncode == 1 and expected in result.stderr and not out.exists(), result
            results[label] = result.stderr.strip()

    build("released")
    assert results["released"] == sealed["standalonePackageLockSha256"]
    explicit = deepcopy(graph)
    explicit["externalRecords"][MANIFEST]["codecMaximum"] = 16384
    build("pinned-inner-limit", g=explicit)
    explicit["externalRecords"][MANIFEST]["codecMaximum"] -= 1
    build("wrong-inner-limit", g=explicit, expected="nested codec maximum")
    for field in ("size", "sha256"):
        altered = deepcopy(overlay)
        altered["externalRecords"][MANIFEST]["fields"][MANIFEST + ":field:" + field] = "other"
        build("wrong-" + field + "-binding", b=altered, expected="nested codec member")
    capped = deepcopy(graph)
    slot = next(slot for slot in capped["layout"] if slot["encoding"].get("record") == MANIFEST)
    slot["encoding"]["maximum"] = 1
    build("impossible-slot", g=capped, expected="nested codec minimum")
    other = deepcopy(overlay)
    other["externalRecords"][MANIFEST]["path"] = "crate::copy_protocol::HelperContext"
    build("wrong-record", b=other, expected="nested codec identity")
    return {"releaseSha256": sealed["inputSha256"], "results": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framework", type=Path, help="Oddlang Git checkout containing the compiler pin")
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory)
        compiler = work / "compiler"
        compiler.mkdir()
        archive = subprocess.check_output(["git", "-C", args.framework, "archive", FRAMEWORK])
        subprocess.run(["tar", "-xf", "-", "-C", compiler], input=archive, check=True)
        subprocess.run(["cargo", "+1.96.1", "build", "--locked", "--offline", "--target-dir", work / "target",
                        "--bin", "oddlang"], cwd=compiler, check=True)
        staged = work / "inputs"
        staged.mkdir()
        os.environ["RUSTUP_TOOLCHAIN"] = "1.97.1"
        report = verify(work / "target/debug/oddlang", staged)
        report["framework"] = FRAMEWORK
    args.receipt.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
