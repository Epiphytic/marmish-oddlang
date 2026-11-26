"""Run independent, stateful callers against a built window library and the pinned original."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import random
import re
import shutil
import subprocess
import tempfile

HERE=Path(__file__).resolve().parent
PIN="af54339d250290490d62d4475a77539b7e739071"
PUBLIC=["bounded_output_reclaims_only_consumed_prefix","exact_retry_conflict_gap_eof_and_partial_delivery"]
WHITEBOX=["many_windows_remain_binary_exact_and_bounded","receipt_memory_and_tiny_chunk_counts_are_bounded","boundary_failures_do_not_mutate_windows"]


def cmd(*args,cwd=None,data=None):
    proc=subprocess.run(args,cwd=cwd,input=data,text=True,capture_output=True)
    if proc.returncode: raise RuntimeError(proc.stdout+proc.stderr)
    return proc.stdout


def compile_caller(work,crate,lock,suite):
    (work/"src").mkdir(parents=True);(work/"tests").mkdir()
    shutil.copyfile(HERE/"caller.rs",work/"src/main.rs")
    (work/"tests/contracts.rs").write_bytes(suite)
    (work/"Cargo.toml").write_text('[package]\nname="window-caller"\nversion="0.1.0"\nedition="2024"\n'
        '[dependencies]\nserde_json="=1.0.151"\nmarmish-stream={path='+json.dumps(str(crate))+'}\n')
    shutil.copyfile(lock,work/"Cargo.lock")
    cmd("cargo","+1.97.1","metadata","--offline","--format-version=1",cwd=work)
    cmd("cargo","+1.97.1","build","--offline","--locked","--release",cwd=work)
    log=cmd("cargo","+1.97.1","test","--offline","--release","--locked","--test","contracts",cwd=work)
    if any(f"test {n} ... ok" not in log for n in PUBLIC):raise ValueError("upstream public test missing")
    return work/"target/release/window-caller"


def traces():
    cases=json.loads(gzip.decompress((HERE/"frozen-cases.json.gz").read_bytes()))
    rng=random.Random(0x507204)
    for role in ("send_window","receive_window"):
        source=role=="send_window"
        for seed in range(48):
            tape=[]; high=0
            for n in range(64):
                cursor=rng.choice([0,high,high+1,max(0,high-1),2**63-1,2**64-1])
                size=rng.choice([0,1,2,3,31,70])
                op=rng.randrange(10)
                if op < 4:
                    item=dict(op="push" if source else "accept",bytes=rng.randbytes(size).hex())
                    if not source: item['offset']=high if op%2==0 else cursor
                    if size and (source or item['offset']==high):high+=size
                elif op < 7:
                    item=dict(op="read" if source else "pending",limit=rng.choice([0,1,2,70,8192,8193]))
                    if source:item['offset']=cursor
                elif op < 9:
                    item=dict(op="ack" if source else "deliver",offset=cursor)
                else:item=dict(op="finish",offset=cursor)
                tape.extend([item,{"op":"state"}])
            cases.append(dict(name=f"{role}-actions-{seed}",operation=role,input=tape))
    return cases


def main():
    ap=argparse.ArgumentParser()
    for k in ("original","generated","receipt"):ap.add_argument(k,type=Path)
    args=ap.parse_args();original=args.original.resolve();generated=args.generated.resolve()
    if cmd("git","rev-parse","HEAD",cwd=original).strip()!=PIN or cmd("git","status","--porcelain",cwd=original):
        raise ValueError("expected clean original")
    src=(original/"crates/marmish-stream/src/windows.rs").read_bytes()
    public=b"use marmish_stream::{Error, MAX_BLOCK, WINDOW_BYTES};\nuse marmish_stream::windows::*;\n"
    for n in PUBLIC:
        fragment=re.search(rb"(?ms)^    #\[test\]\n    fn "+n.encode()+rb"\(\) \{.*?^    \}\n",src)
        if not fragment:raise ValueError(n)
        public+=fragment.group(0)
    cases=traces()
    payload="".join(json.dumps(t,separators=(",",":"))+"\n" for t in cases)
    with tempfile.TemporaryDirectory() as td:
        work=Path(td)
        ref=compile_caller(work/"reference",original/"crates/marmish-stream",original/"Cargo.lock",public)
        got=compile_caller(work/"candidate",generated,generated/"Cargo.lock",public)
        def observe(binary):
            result=[json.loads(s) for s in cmd(binary,data=payload).splitlines()]
            if [n for n,_ in result]!=[case["name"] for case in cases]:raise ValueError("trace count/order differs")
            return result
        rows=observe(ref);other=observe(got)
        for case,(_,a),(_,b) in zip(cases,rows,other,strict=True):
            if a!=b or a!=case.get("expected",a):raise ValueError(f"stateful trace diverged: {case['name']}")
        args.receipt.write_text(json.dumps(dict(source=PIN,publicUpstreamTests=PUBLIC,privateStructuralTests=WHITEBOX,
            frozenTraces=sum("expected" in t for t in cases),seededTraces=sum("expected" not in t for t in cases),
            operationResults=sum(len(t["input"]) for t in cases),packageLockSha256=hashlib.sha256((generated/"package.lock").read_bytes()).hexdigest(),
            transcriptSha256=hashlib.sha256(json.dumps(rows,separators=(",",":")).encode()).hexdigest()),indent=2)+"\n")


if __name__=="__main__":main()
