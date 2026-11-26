"""Verify the frozen inventory against the independent release-name oracle and pin."""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

here = Path(__file__).resolve().parent
repo = Path(sys.argv[1]).resolve()
spec_path = here / "release.json"
spec = json.loads(spec_path.read_bytes())
with tempfile.TemporaryDirectory() as temporary:
    stage = Path(temporary)
    generated = stage / "inventory.json"
    subprocess.run([sys.executable, here / "inventory.py", repo, spec_path, generated], check=True)
    if generated.read_bytes() != (here / "inventory.json").read_bytes():
        sys.exit("inventory drift")
    graph = stage / "inventory.ttl"
    subprocess.run([sys.executable, here / "graph.py", spec_path, generated, graph], check=True)
    if graph.read_bytes() != (here / "inventory.ttl").read_bytes():
        sys.exit("inventory graph drift")
    inventory = json.loads(generated.read_bytes())
    # Isolate the independently maintained, dependency-free ES module from the precise Git object.
    module = stage / "release-schema.mjs"
    module.write_bytes(subprocess.check_output(
        ["git", "show", spec["commit"] + ":tools/v02-actions/release-schema.mjs"], cwd=repo))
    package = next(p for w in inventory["workspaces"] for p in w["packages"]
                   if p["name"] == spec["release"]["package"])
    script = "import {releaseNames} from './release-schema.mjs'; console.log(JSON.stringify(releaseNames(process.argv[1])))"
    names = json.loads(subprocess.check_output(["node", "--input-type=module", "-e", script,
                                                package["version"]], cwd=stage))
    if names != inventory["release_assets"]:
        sys.exit("independent upstream split asset list differs")
    spec["workspaces"] = spec["workspaces"][:1]
    mutation = stage / "omitted.json"
    mutation.write_text(json.dumps(spec))
    result = subprocess.run([sys.executable, here / "inventory.py", repo, mutation, generated],
                            capture_output=True, text=True)
    if not result.returncode or "unclassified Cargo manifests" not in result.stderr:
        sys.exit("failed to detect omission of the standalone fixture workspace")
print("Reproduced inventory, matched upstream asset list, detected omitted workspace.")
