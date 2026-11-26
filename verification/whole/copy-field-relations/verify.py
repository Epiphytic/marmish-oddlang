"""Compare match and checked context relations against an independent portable oracle."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent/"copy-launch"))
from inputs import PIN, ROOT, consumer, observe, read, run, sha, write
from verify import project, staged

CALLER=HERE/"caller.rs"
MANIFEST=ROOT/"oddlang/portable/stream-copy-field-relations-package.json"
FRAMEWORK="7588c8288b89e4a4abc7cfbd110c9b71305377f7"
manifest=runpy.run_path(str(HERE.parent/"copy-manifest/verify.py"))
context=runpy.run_path(str(HERE.parent/"copy-helper-context/verify.py"))
launch_hash=runpy.run_path(str(HERE.parent/"copy-launch-hash/verify.py"))
KEYS=("version","transfer","client","server","direction","source","destination","overwrite","preserve_mode","preserve_mtime")
IDENTITIES=("client","server","transfer","direction")


def launch_wire(launch):
    value={key:launch[key] for key in (*KEYS,"mode")}
    for key in ("client","server","transfer"):
        value[key]=bytes(value[key]).hex()
    for key in ("source","destination"):
        value[key]=base64.b64encode(bytes(value[key])).decode()
    return value


def corpus():
    base=manifest["corpus"]()[0][0]
    launch={key:base[key] for key in KEYS} | {"mode":"create"}
    helper=context["corpus"]()[0][0]
    helper.update({key:launch[key] for key in IDENTITIES})
    requests,expected=[],[]

    def match(value, record=base, **force):
        decoded=record.copy()
        for key in ("source","version"):
            if "force_"+key in force:
                decoded[key]=force["force_"+key]
        requests.append(dict(operation="match",launch=launch_wire(value),manifest=list(manifest["encode"](record)),**force))
        expected.append(all(value[key]==decoded[key] for key in KEYS))

    def start(value, codec="none", observed=helper, invalid_context=False):
        requests.append(dict(operation="start",context=list(context["encode"](observed)),invalid_context=invalid_context,
                             launch=launch_wire(value),codec=codec))
        if invalid_context:
            expected.append(context["error"]("copy_context"))
        elif any(value[key]!=observed[key] for key in IDENTITIES):
            expected.append(context["error"]("copy_context_launch"))
        else:
            digest=launch_hash["expected"](dict(value,codec=codec))["digest"]
            if "error" in digest:
                expected.append(digest)
            elif digest["ok"] != observed["launch_digest"]:
                expected.append(context["error"]("copy_context_launch"))
            else:
                expected.append({"ok":None})

    def build(observed=helper, invalid_context=False, **identity):
        actual=dict(version=observed["expected_version"],target=observed["expected_target"],commit=observed["expected_commit"])
        actual.update(identity)
        requests.append(dict(operation="build",context=list(context["encode"](observed)),
                             invalid_context=invalid_context,**actual))
        if invalid_context:
            expected.append(context["error"]("copy_context"))
        elif any(actual[key] != observed["expected_"+key] for key in actual):
            expected.append(context["error"]("copy_context_build"))
        else:
            expected.append({"ok":None})

    match(launch)
    match(dict(launch,mode="resume"),dict(base,mode=0o700,size=0,sha256=[11]*32,mtime={"seconds":99,"nanos":5}))
    match(dict(launch,version=2),force_version=2)
    match(dict(launch,source=[0]),force_source=[0])
    diffs=dict(version=2,transfer=[5]*32,client=[6]*32,server=[7]*32,direction="get",
               source=[12],destination=[13],overwrite=False,preserve_mode=False,preserve_mtime=False)
    for key,value in diffs.items():
        match(dict(launch,**{key:value}))

    for mode in ("create","resume"):
        for codec in ("none","zlib"):
            entry=dict(launch,mode=mode)
            digest=launch_hash["expected"](dict(entry,codec=codec))["digest"]["ok"]
            observed=dict(helper,launch_digest=digest)
            start(entry,codec,observed)
            start(entry,codec,observed,invalid_context=True)
            start(entry,"zlib" if codec=="none" else "none",observed)
            start(dict(entry,version=0,source=[0]),codec,observed)
            start(dict(entry,source=[0]),codec,observed)
            start(dict(entry,destination=[]),codec,observed)
            for key in IDENTITIES:
                start(dict(entry,**{key:diffs[key]},source=[0]),codec,observed)
                start(dict(entry,**{key:diffs[key]},source=[0]),codec,observed,invalid_context=True)

    for invalid in (False,True):
        build(invalid_context=invalid)
        for key, candidates in (("version",["","v1_.A","é","v/1","v"*65]),
                                 ("target",["","Aarch64-apple-darwin","x"*65]),
                                 ("commit",[[0]*20,[111]*19+[1]])):
            for value in candidates:
                build(invalid_context=invalid,**{key:value})
        build(invalid_context=invalid,version="?",target="!",commit=[0]*20)
    return requests,expected


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for label in ("original","framework","receipt"):
        parser.add_argument(label,type=Path)
    args=parser.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    for checkout,pin in ((original,PIN),(framework,FRAMEWORK)):
        assert run("git","rev-parse","HEAD",cwd=checkout)==pin
        assert not run("git","status","--porcelain",cwd=checkout)

    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=framework)
        cli=work/"tools/debug/oddlang"
        manifest_pin=staged(work,MANIFEST)
        native=work/"native"
        sandbox=project(work,cli,manifest_pin,native,original,CALLER)
        lock=native/"package.lock"
        assert lock.read_bytes()==(MANIFEST.parent/"copy-field-relations-v1/package.lock").read_bytes()
        pinned=consumer(original/"crates/marmish-stream",work/"original",original/"Cargo.lock",CALLER)
        projected=consumer(native,work/"generated",native/"Cargo.lock",CALLER)
        requests,expected=corpus()
        for library,exe in (("original",pinned),("generated",projected)):
            for index,(got,want) in enumerate(zip(observe(exe,requests),expected,strict=True)):
                assert got==want,(library,index,got,want)

        mutant=read(manifest_pin)
        pin=mutant["components"][-1]["graph"]
        graph_path=manifest_pin.parent/pin["path"]
        graph=read(graph_path)
        start,=[operation for key,operation in graph["relations"].items() if key.endswith(":validate-start")]
        start["steps"].insert(1,start["steps"].pop())
        write(graph_path,graph)
        pin["sha256"]=sha(graph_path)
        write(manifest_pin,mutant)
        mutated=work/"mutated"
        project(work,cli,manifest_pin,mutated,original,CALLER)
        observer=consumer(mutated,work/"mutation",mutated/"Cargo.lock",CALLER)
        req=next(req for req,result in zip(requests,expected) if req["operation"]=="start"
                 and req["launch"]["source"]=="AA==" and not req["invalid_context"]
                 and result==context["error"]("copy_context_launch"))
        assert observe(observer,[req])==[context["error"]("copy_path")]

        counts={name:sum(req["operation"]==name for req in requests) for name in ("match","start","build")}
        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
              originalAndCompanionReadDenied=sandbox, packageLockSha256=sha(lock),callerSha256=sha(CALLER),
              cases=counts,observationsSha256=hashlib.sha256(json.dumps(expected,separators=(",",":")).encode()).hexdigest(),
              negativeControl="advancing the fallible digest before identity checks converts copy_context_launch into copy_path"))
        print(f"Qualified native copy relations {counts}.")


if __name__=="__main__":
    main()
