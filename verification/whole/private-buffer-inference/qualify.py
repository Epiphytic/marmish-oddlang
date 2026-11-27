"""Infer behavior from native observations, then independently replay the generated reader."""
import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
CASE = "portable_buffer_behavior::collect"
LANES = {"original": [], "empty": ["emptyRead"], "consume": ["consumedBytes"],
         "error": ["refillError"], "combined": ["emptyRead", "consumedBytes", "refillError"]}


sys.path.insert(0,str(HERE.parent/"private-buffer"))
import verify as buffer
from verify import PIN, SOURCE

product_witness = runpy.run_path(str(HERE.parent / "unified-mdk/workspace_compat.py"))["product_witness"]

SELECTION = HERE/"shape-selection.json"
EXPECTED_CHANGES = {"emptyRead":"return-zero", "consumedBytes":"retain", "refillError":"discard-window"}


def digest(data):
    return sha256(data).hexdigest()


def shape(original, framework, target):
    return subprocess.check_output(["bash",framework/"verification/rust-recovery/run.sh",original/"crates/marmish-prototype/Cargo.toml",
                                   "marmish-prototype","lib","marmish_prototype",target,"default"],
                                  env=dict(os.environ,ODD_RECOVERY_DATA_SHAPES="1"))


def selected(raw):
    selector = json.loads(SELECTION.read_bytes())
    decl, = [d for d in json.loads(raw)["graph"]["declarations"] if d["id"]==selector["declaration"]]
    return decl


def qualify(original,framework,cli,target,original_cargo_target):
    native = product_witness("buffer-traces", original, original_cargo_target)
    if native != json.loads((HERE/f"original-{target}.json").read_bytes()):
        raise ValueError("live source behavior differs from the pinned observations")
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory).resolve()
        changed = work/"reference-mutant"
        mutated = product_witness("buffer-trace-mutation", original, changed)
        if mutated != native["lanes"]["combined"]["sourceSha256"]:
            raise ValueError("mutated shape and execution source disagree")
        raw = shape(original,framework,target)
        altered = shape(changed,framework,target)
        if selected(raw) != selected(altered):
            raise ValueError("combined behavior mutations changed the storage declaration")
        shape_path = work/"shapes.json"
        shape_path.write_bytes(raw)
        base = json.loads(buffer.GRAPH.read_bytes())
        receipts = {}
        for name,changes in LANES.items():
            lane = work/name
            lane.mkdir()
            trace = native["lanes"][name]["trace"]
            evidence = lane/"trace.json"
            evidence.write_text(json.dumps(trace,indent=2)+"\n")
            out = lane/"inferred"
            subprocess.run([cli,"recover","observed-buffered-input",shape_path,SELECTION,evidence,out],check=True)
            graph = (out/"graph.json").read_bytes()
            proof = json.loads((out/"recovery.lock").read_bytes())
            for key,raw_input in [("graphSha256",graph),("traceSha256",evidence.read_bytes()),
                                  ("selectionSha256",SELECTION.read_bytes()),("observationSha256",raw),
                                  ("overlaySha256",(out/"rust.json").read_bytes())]:
                if proof[key]!=digest(raw_input):
                    raise ValueError(f"{name}: recovery receipt differs for {key}")
            expected = dict(base,transitions={**base["transitions"],**{k:EXPECTED_CHANGES[k] for k in changes}})
            if (json.loads(graph)!=expected or (out/"rust.json").read_bytes()!=buffer.OVERLAY.read_bytes()
                    or (not changes and graph!=buffer.GRAPH.read_bytes())):
                raise ValueError(f"{name}: inferred contract differs")
            manifest = json.loads(buffer.MANIFEST.read_bytes())
            part, = manifest["components"]
            with tempfile.TemporaryDirectory(dir="/tmp" if sys.platform=="linux" else None) as tmp:
                sealed = Path(tmp).resolve()
                inputs = sealed/"inputs"
                inputs.mkdir()
                for pin in [manifest["cargoLock"],part["graph"],part["overlay"]]:
                    original_input = (buffer.MANIFEST.parent/pin["path"]).read_bytes()
                    if digest(original_input)!=pin["sha256"]:
                        raise ValueError("invalid product input")
                    data = graph if pin is part["graph"] else original_input
                    dest = inputs/pin["path"]
                    dest.parent.mkdir(parents=True,exist_ok=True)
                    dest.write_bytes(data)
                    pin["sha256"] = digest(data)
                (inputs/buffer.MANIFEST.name).write_bytes(buffer.MANIFEST.read_bytes() if not changes else
                                                         json.dumps(manifest,indent=2).encode()+b"\n")
                built,lock = buffer.project(sealed,inputs,cli,original,framework,target,[evidence,shape_path,changed/SOURCE,HERE/"trace.rs"])
                product = lane/"product"
                shutil.move(str(built),product)
            with (product/"src/group_0/__projection_0.rs").open("ab") as leaf:
                leaf.write(b"\n"+(HERE/"trace.rs").read_bytes())
            emitted = product_witness("buffer-trace-case", product, lane/"test-target", target, buffer.NATIVE_TEST_PREFIX+CASE)
            if emitted!=trace:raise ValueError(f"{name}: generated native behavior differs")
            receipts[name] = dict(graphSha256=digest(graph),packageLockSha256=lock,
                                  recoveryLockSha256=digest((out/"recovery.lock").read_bytes()),
                                  traceSha256=digest(evidence.read_bytes()),sourceSha256=native["lanes"][name]["sourceSha256"],
                                  selectionSha256=digest(SELECTION.read_bytes()),generatedTraceMatched=True)
        return dict(original=PIN,target=target,originalObservationsSha256=digest((HERE/f"original-{target}.json").read_bytes()),
                    shapeUnchangedUnderCombinedMutation=True,lanes=receipts)


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    for n in ("original","framework","compiler_cache","original_cargo_target","receipt"):
        ap.add_argument(n,type=Path)
    args=ap.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    code=json.loads((HERE/"compiler.json").read_bytes())["revision"]
    for root,rev in [(original,PIN),(framework,code)]:
        if buffer.run("git","-C",root,"rev-parse","HEAD").decode().strip()!=rev or buffer.run("git","-C",root,"status","--porcelain"):
            raise ValueError(f"expected clean checkout at {root}")
    subprocess.run(["cargo","+1.96.1","build","--locked","--offline","--bin","oddlang","--target-dir",args.compiler_cache],
                   cwd=framework,env=buffer.CORE["cargo_environment"](),check=True)
    host=next(l[6:] for l in buffer.run("rustc","+1.97.1","-vV").decode().splitlines() if l.startswith("host: "))
    result=qualify(original,framework,args.compiler_cache.resolve()/"debug/oddlang",host,args.original_cargo_target.resolve())
    result["framework"]=code
    args.receipt.write_text(json.dumps(result,indent=2)+"\n")
