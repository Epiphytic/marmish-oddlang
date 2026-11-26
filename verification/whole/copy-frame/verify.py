"""Build sealed copy frames and run unchanged original tests on both native libraries."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import shutil
import sys
import tempfile

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/"copy-launch"))
from inputs import PIN,ROOT,consumer,observe,run,read,sha,write
from verify import project,staged

PACKAGE=ROOT/"oddlang/portable/stream-copy-frame-package.json"
FRAMEWORK="20f26a01bda33a1c6a6b284839dbf417f88af850"
EXACT=HERE/"exact.rs"
EXPECTED_UPSTREAM=11
EXPECTED_EXACT=9


def tests(package,lock,dest,original_tests,exact=EXACT,negative=False):
    source=dest/"src"
    source.mkdir(parents=True)
    (source/"upstream.rs").write_bytes(original_tests)
    shutil.copyfile(exact,source/"exact.rs")
    (source/"lib.rs").write_text('''
#[cfg(test)]
mod copy_protocol {
    // The pinned tests import their parent scope with use super::*.
    pub use marmish_stream::{MAX_BLOCK, MAX_OFFSET, codec::Codec, copy_protocol::*};
    pub use sha2::{Digest,Sha256};
    const CONTEXT_DOMAIN: &[u8] = b"marmish-copy-context/1\\0";
    include!("upstream.rs");
}
#[cfg(test)]
mod exact;
''')
    (dest/"Cargo.toml").write_text(
        '[package]\nname="verify-copy-frame"\nversion="0.0.0"\nedition="2021"\n'
        '[dependencies]\nsha2="=0.10.9"\nmarmish-stream={path='+
        json.dumps(str(package.resolve()))+'}\n[workspace]\n'
    )
    shutil.copyfile(lock,dest/"Cargo.lock")
    run("cargo","+1.97.1","metadata","--offline","--format-version","1",cwd=dest)
    if negative:
        import subprocess
        for name in ("data_and_progress_guards_have_directional_precedence",
                     "data_decode_rejects_offset_before_empty_body"):
            r=subprocess.run(["cargo","+1.97.1","test","--locked","--offline","--release",f"exact::{name}"],
                             cwd=dest,capture_output=True,text=True)
            assert r.returncode and "Offset" in r.stdout and "Limit" in r.stdout,(r.stdout,r.stderr)
        return []
    output=run("cargo","+1.97.1","test","--locked","--offline","--release","--lib",cwd=dest)
    cases=sorted(re.findall(r"^test (\S+) \.\.\. ok$",output,re.M))
    assert len(cases)==EXPECTED_UPSTREAM+EXPECTED_EXACT,output
    assert sum(case.startswith("copy_protocol::tests::") for case in cases)==EXPECTED_UPSTREAM
    return cases


def repeat_record_evidence(package,work):
    counts={}
    for name in ("manifest","checkpoint"):
        v=runpy.run_path(str(HERE.parent/f"copy-{name}/verify.py"))
        constructions,wire=v["corpus"]()
        requests=[dict(operation="construct",**{("manifest" if name=="manifest" else "record"):value})
                  for value in constructions]
        requests.extend(dict(operation="decode",bytes=list(data)) for data in wire)
        expected=[]
        for value in constructions:
            error=v["validate"](value)
            if name=="manifest":
                error=v["fault"](error) if error else None
                expected.append(dict(validation=error or {"ok":None},encode=error or {"ok":list(v["encode"](value))},
                                     namespace=v["namespace"](value)))
            else:
                expected.append(dict(validation=error or {"ok":None},encoding=error or {"ok":list(v["encode"](value))}))
        expected.extend(v["decode"](data) for data in wire)
        exe=consumer(package,work/name,package/"Cargo.lock",v["CALLER"])
        for index,(actual,want) in enumerate(zip(observe(exe,requests),expected,strict=True)):
            assert actual==want,(name,index,actual,want)
        counts[name]={"constructions":len(constructions),"decodes":len(wire)}
    return counts


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","receipt"):
        parser.add_argument(name,type=Path)
    args=parser.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    for checkout,pin in ((original,PIN),(framework,FRAMEWORK)):
        assert run("git","rev-parse","HEAD",cwd=checkout)==pin
        assert not run("git","status","--porcelain",cwd=checkout)
    upstream=(original/"crates/marmish-stream/src/copy_protocol.rs").read_bytes()
    marker=b"#[cfg(test)]\nmod tests {"
    index=upstream.index(marker)
    end=upstream.index(b"\nmod path_encoding {",index)
    original_tests=upstream[index:end]
    assert original_tests.count(b"#[test]")==EXPECTED_UPSTREAM
    with tempfile.TemporaryDirectory() as directory:
        work=Path(directory).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=framework)
        cli=work/"tools/debug/oddlang"
        staged_manifest=staged(work,PACKAGE)
        native=work/"native"
        sandbox=project(work,cli,staged_manifest,native,original,EXACT)
        lock=native/"package.lock"
        assert lock.read_bytes()==(PACKAGE.parent/"copy-frame-v1/package.lock").read_bytes()
        reference=tests(original/"crates/marmish-stream",original/"Cargo.lock",work/"original",original_tests)
        generated=tests(native,native/"Cargo.lock",work/"generated",original_tests)
        assert reference==generated
        repeated=repeat_record_evidence(native,work/"nested")

        mutant=read(staged_manifest)
        pin=next(c["graph"] for c in mutant["components"] if c["kind"]=="binary-frame")
        graph_path=staged_manifest.parent/pin["path"]
        graph=read(graph_path)
        data=next(case for case in graph["cases"].values() if case["tag"]==5)
        data["encodeGuards"].reverse()
        offset_slot=data["slots"][0]
        offset_slot["encoding"]["maximum"]=2**64-1
        write(graph_path,graph)
        pin["sha256"]=sha(graph_path)
        write(staged_manifest,mutant)
        variant=work/"mutated"
        project(work,cli,staged_manifest,variant,original,EXACT)
        tests(variant,variant/"Cargo.lock",work/"negative",original_tests,negative=True)

        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
             originalAndCompanionReadDenied=sandbox,packageLockSha256=sha(lock),
             upstreamTestModuleSha256=hashlib.sha256(original_tests).hexdigest(),exactTestsSha256=sha(EXACT),
             upstreamTests=EXPECTED_UPSTREAM,exactTests=EXPECTED_EXACT,testNames=generated,
             repeatedStandaloneEvidence=repeated,
             negativeControl="reordering Data encode guards and deferring the early offset bound reverse exact Offset and Limit precedence"))
        print(f"Qualified {EXPECTED_UPSTREAM} unchanged upstream tests and {EXPECTED_EXACT} exact-error tests on both native libraries.")


if __name__=="__main__":
    main()
