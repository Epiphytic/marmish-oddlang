"""Qualify a generated stream codec against its pinned native original."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from cases import qualify, send

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]/"oddlang/portable"
PIN="af54339d250290490d62d4475a77539b7e739071"
TESTS=("roundtrips_and_fallback","incompressible_maximum_block_uses_none",
       "rejects_corruption_truncation_frames_and_dictionary",
       "rejects_nonbeneficial_zlib_representation","rejects_bounds_and_length")


def run(*args,cwd=None):
    p=subprocess.run(args,cwd=cwd,capture_output=True,text=True)
    if p.returncode:
        raise RuntimeError(f"{args} failed: {p.stdout}{p.stderr}")
    return p.stdout


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def consumer(crate, work, original_lock, suite):
    work.mkdir()
    (work/"src").mkdir()
    (work/"tests").mkdir()
    shutil.copyfile(HERE/"caller.rs",work/"src/main.rs")
    (work/"tests/contracts.rs").write_bytes(
        b"use marmish_stream::{Error, MAX_BLOCK, MAX_OFFSET};\nuse marmish_stream::codec::*;\n"+suite)
    manifest=('[package]\nname="exercise-block"\nversion="0.1.0"\nedition="2024"\n'
              '[dependencies]\nserde_json="=1.0.151"\nmarmish-stream={path='+json.dumps(str(crate))+'}\n')
    (work/"Cargo.toml").write_text(manifest)
    shutil.copyfile(original_lock,work/"Cargo.lock")
    # Cargo removes dependencies unrelated to this standalone caller before locked builds.
    run("cargo","+1.97.1","metadata","--offline","--format-version=1",cwd=work)
    run("cargo","+1.97.1","build","--locked","--offline","--release",cwd=work)
    output=run("cargo","+1.97.1","test","--locked","--offline","--release","--test","contracts",cwd=work)
    for test in TESTS:
        if f"tests::{test} ... ok" not in output: raise ValueError(f"missing original test {test}")
    return work/"target/release/exercise-block"


def main():
    ap=argparse.ArgumentParser()
    for name in ("original","oddlang","recovery","receipt"): ap.add_argument(name,type=Path)
    opts=ap.parse_args()
    original=opts.original.resolve()
    if (run("git","rev-parse","HEAD",cwd=original).strip()!=PIN or
            run("git","status","--porcelain",cwd=original)):
        raise ValueError("expected clean pinned original")
    lib=ROOT/"stream-shared-v1"
    with tempfile.TemporaryDirectory() as td:
        stage=Path(td); graph=stage/"inputs";graph.mkdir()
        p=ROOT/"stream-block-package.json"; m=json.loads(p.read_bytes())
        shutil.copyfile(p,graph/p.name)
        for pin in [m["cargoLock"]]+[c[k] for c in m["components"] for k in ("graph","overlay")]:
            src=(ROOT/pin["path"]).resolve()
            if not src.is_relative_to(ROOT) or sha(src)!=pin["sha256"]: raise ValueError("invalid manifest pin")
            dest=graph/src.relative_to(ROOT);dest.parent.mkdir(exist_ok=True);shutil.copyfile(src,dest)
        projected=stage/"projected"
        run(opts.oddlang.resolve(),"build","native-package","rust",graph/p.name,projected,cwd=stage)
        if (projected/"package.lock").read_bytes()!=(ROOT/"stream-block-package-v1/package.lock").read_bytes():
            raise ValueError("regenerated package lock differs from the committed native projection")
        run("cargo","+1.97.1","build","--offline","--locked","--release",cwd=projected)
        recovered=stage/"observed"
        run(opts.recovery.resolve(),"recover","shared-declarations",original,lib/"selection.json",recovered)
        for name in ("graph.json","rust.json","observation.json"):
            if (recovered/name).read_bytes()!=(lib/name).read_bytes(): raise ValueError(f"recovery differs in {name}")
        marker=b"#[cfg(test)]\nmod tests {"
        upstream=run("git","show",PIN+":crates/marmish-stream/src/codec.rs",cwd=original).encode()
        if upstream.count(marker)!=1: raise ValueError("unchanged suite could not be located")
        suite=upstream[upstream.index(marker):]
        left=consumer(original/"crates/marmish-stream",stage/"original",original/"Cargo.lock",suite)
        right=consumer(projected,stage/"generated",projected/"Cargo.lock",suite)
        result=qualify(left,right)
        # Keep all native ABI names and dependency versions. Change one semantic bound.
        component=m["components"][1]["graph"]
        policy_path=graph/component["path"]
        policy=json.loads(policy_path.read_bytes())
        policy["bounds"]["offsetMaximum"]-=1
        policy_path.write_text(json.dumps(policy))
        component["sha256"]=sha(policy_path)
        (graph/p.name).write_text(json.dumps(m))
        mutant=stage/"changed"
        run(opts.oddlang.resolve(),"build","native-package","rust",graph/p.name,mutant,cwd=stage)
        # Existing upstream tests use the crate-wide maximum and cover rejecting at the limit.
        # An independently chosen last valid offset distinguishes this narrower accepted range.
        changed=consumer(mutant,stage/"mutant-caller",mutant/"Cargo.lock",suite)
        control={"last-byte":{"operation":"encode","negotiated":"none","offset":2**63-2,"bytes":[0]}}
        original_obs=send(left,control)["last-byte"]
        changed_obs=send(changed,control)["last-byte"]
        if "wire" not in original_obs or changed_obs.get("error")!="Offset":
            raise ValueError("offset mutation did not change the selected outcome")
        result["mutation"]={"offsetMaximum":policy["bounds"]["offsetMaximum"],"error":changed_obs}

        result.update(reference=PIN,sourceSuiteSha256=hashlib.sha256(suite).hexdigest(),upstreamTests=list(TESTS),
                      nativeCompiler=run("rustc","+1.97.1","-vV").splitlines(),packageLockSha256=sha(projected/"package.lock"),
                      publicCallerSha256=sha(HERE/"caller.rs"),manifestSha256=sha(p))
        opts.receipt.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")


if __name__=="__main__": main()
