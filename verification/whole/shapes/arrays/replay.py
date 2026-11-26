"""Recover the stream graph and compare its array dimensions to the previous release and rustdoc."""
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from compare import compare
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import workspace_shapes as archive

FRAMEWORK = "ce22923ccc3f0f34b71c08525b455c16bc568d4e"


def run(original, framework, output):
    archive.checkout(original, archive.UPSTREAM)
    archive.checkout(framework, FRAMEWORK)
    host = archive.capture(original, "rustc", "+1.97.1", "-vV").split("host: ")[1].splitlines()[0]
    release = json.loads((archive.ARCHIVE / "release.json").read_bytes())
    cell, = [v for v in release["cells"] if (v["package"], v["kind"], v["features"], v["triple"])
             == ("marmish-stream", "lib", [], host)]
    old = archive.ARCHIVE / "graphs" / (cell["graphSha256"] + ".json")
    if archive.sha(old) != cell["graphSha256"]:
        raise ValueError("previous archived graph digest mismatch")
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory)
        profile = archive.pin(original, cell)
        for key in ("packageManifestSha256", "manifestSha256"):
            del profile[key]
        profile.update(adapter=str(framework / archive.ADAPTER / "run.sh"), adapterInputs=archive.INPUTS,
                       sourceRevision=archive.UPSTREAM)
        selection = work / "profile.toml"
        selection.write_text("".join(f"{k} = {json.dumps(v)}\n" for k,v in profile.items()))
        subprocess.run(["cargo", "+1.96.1", "build", "--offline", "--locked", "--bin", "oddlang",
                        "--target-dir", work / "tools"], cwd=framework, check=True)
        subprocess.run([work / "tools/debug/oddlang", "recover", "rust-shapes", original / "Cargo.toml",
                        "--profile", selection, "--out", work / "shapes"], cwd=original, check=True)
        subprocess.run(["cargo", "+1.97.1", "rustdoc", "--offline", "--locked", "--release", "--lib",
                        "-p", cell["package"], "--target", host, "--target-dir", work / "doc-build",
                        "--", "-Z", "unstable-options", "--output-format", "json", "--document-private-items"],
                       cwd=original, env=dict(os.environ, RUSTC_BOOTSTRAP="1"), check=True)
        doc = json.loads((work / "doc-build" / host / "doc/marmish_stream.json").read_bytes())
        graph_path = work / "shapes/shapes.json"
        graph = json.loads(graph_path.read_bytes())
        previous = json.loads(old.read_bytes())
        report = compare(previous, graph, doc)
        mutant = copy.deepcopy(graph)
        launch, = [d for d in mutant["declarations"] if d["name"] == "Launch"]
        transfer, = [f for f in launch["fields"] if f["name"] == "transfer"]
        transfer["type"]["extent"]["count"] += 1
        try:
            compare(previous, mutant, doc)
        except ValueError as error:
            if "array element or extent mismatch" not in str(error):
                raise
        else:
            raise ValueError("independent field extent mutation passed")
        report.update(upstream=archive.UPSTREAM, framework=FRAMEWORK, host=host,
                      previousGraphSha256=archive.sha(old), graphSha256=archive.sha(graph_path),
                      rustdocFormat=doc["format_version"], mutationRejected=transfer["id"])
        lock = work / "shapes/shapes.lock"
        if json.loads(lock.read_bytes())["shapesSha256"] != report["graphSha256"]:
            raise ValueError("shape lock mismatch")
        output.mkdir()
        for path in (graph_path, lock):
            (output / path.name).write_bytes(path.read_bytes())
        (output / "receipt.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("original", "framework", "output"):
        p.add_argument(arg, type=Path)
    args = p.parse_args()
    run(args.original.resolve(), args.framework.resolve(), args.output.resolve())
