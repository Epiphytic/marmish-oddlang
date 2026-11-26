"""Verify source-free checkpoint framing, identities, nested validation and checksum."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import struct
import sys
import tempfile

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/"copy-launch"))
from inputs import PIN,ROOT,consumer,observe,read,run,sha,write
from verify import project,staged

manifest=runpy.run_path(str(HERE.parent/"copy-manifest/verify.py"))
FRAMEWORK="3b3a1ef0e560041474cf6cddedcd52ab66defde2"
PACKAGE=ROOT/"oddlang/portable/stream-copy-checkpoint-package.json"
CALLER=HERE/"caller.rs"
DOMAIN=b"marmish-copy-checkpoint/1\0"
PHASES=("created","receiving","verifying","commit_prepared","committed")
RETENTION=604860
LIMIT=16384
END=2**63-1
EMPTY=list(hashlib.sha256(b"").digest())
fault=manifest["fault"]


def validate(record):
    payload=manifest["decode"](bytes(record["manifest"]))
    if "error" in payload:
        return payload
    m=payload["ok"]["manifest"]
    version=record["manifest_version"]
    if version is not None and version != 1:
        return fault("copy_version")
    path=record["normalized_path"]
    if not 1<=len(path)<=4104 or 0 in path:
        return fault("copy_local_path")
    offset,phase=record["durable_offset"],record["phase"]
    created,updated,expires=(record[k] for k in ("created","updated","expires"))
    age=expires-updated
    final=record["committed_final"]
    digest=record["prefix_sha256"]
    if (offset>m["size"] or updated<created or not RETENTION<=age<=END
        or (phase=="created" and offset!=0)
        or (phase in PHASES[2:] and offset!=m["size"])
        or (phase=="committed")!=(final is not None)
        or (final is not None and final!=record["data"])
        or (offset==0 and digest!=EMPTY)
        or (offset==m["size"] and digest!=m["sha256"])):
        return fault("copy_checkpoint")
    return None


def seal(payload, domain=DOMAIN):
    return payload+hashlib.sha256(domain+payload).digest()


def encode(record, domain=DOMAIN):
    m=bytes(record["manifest"])
    if record["manifest_version"] is not None:
        m=struct.pack(">I",record["manifest_version"])+m[4:]
    path=bytes(record["normalized_path"])
    final=record["committed_final"]
    return seal(b"".join((struct.pack(">II",1,len(m)),m,bytes([PHASES.index(record["phase"])+1]),
        struct.pack(">I",len(path)),path,
        *(struct.pack(">QQ",*record[k]) for k in ("parent","private","data")),
        struct.pack(">Q",record["durable_offset"]),bytes(record["prefix_sha256"]),bytes(record["last_stream"]),
        struct.pack(">qqq",*(record[k] for k in ("created","updated","expires"))),
        b"\0" if final is None else b"\1"+struct.pack(">QQ",*final))),domain)


def decode(wire):
    if not 32<=len(wire)<=LIMIT:
        return fault("Limit")
    body,check=wire[:-32],wire[-32:]
    if hashlib.sha256(DOMAIN+body).digest()!=check:
        return fault("copy_checkpoint_checksum")
    pos=0
    def take(length):
        nonlocal pos
        if pos+length>len(body):
            raise ValueError("copy_truncated")
        out=body[pos:pos+length]
        pos+=length
        return out
    def number(length):
        return int.from_bytes(take(length),"big")
    def bounded(limit):
        n=number(4)
        if n>limit:
            raise ValueError("Limit")
        return take(n)

    try:
        if number(4)!=1:
            raise ValueError("copy_checkpoint_version")
        m=bounded(LIMIT)
        if "error" in (out:=manifest["decode"](m)):
            return out
        phase=number(1)
        if not 1<=phase<=len(PHASES):
            raise ValueError("copy_enum")
        path=bounded(4104)
        ids={k:list(struct.unpack(">QQ",take(16))) for k in ("parent","private","data")}
        offset=number(8)
        if offset>END:
            raise ValueError("Offset")
        prefix=take(32)
        stream=take(16)
        created,updated,expires=struct.unpack(">qqq",take(24))
        tag=number(1)
        if tag>1:
            raise ValueError("copy_option")
        final=list(struct.unpack(">QQ",take(16))) if tag else None
        if pos!=len(body):
            raise ValueError("copy_trailing")
        record=dict(manifest=list(m),manifest_version=None,phase=PHASES[phase-1],normalized_path=list(path),
                    **ids,durable_offset=offset,prefix_sha256=list(prefix),last_stream=list(stream),created=created,
                    updated=updated,expires=expires,committed_final=final)
        return validate(record) or {"ok":record}
    except ValueError as error:
        return fault(str(error))


def corpus():
    m=dict(manifest["corpus"]()[0][0],size=3)
    base=dict(manifest=list(manifest["encode"](m)),manifest_version=None,phase="receiving",
              normalized_path=[250,11,1],parent=[0,2**64-1],private=[3,4],data=[5,6],durable_offset=0,
              prefix_sha256=EMPTY,last_stream=[20]*16,created=-77,updated=0,expires=RETENTION,committed_final=None)
    values=[base,dict(base,manifest_version=0,normalized_path=[],durable_offset=END+1),
            dict(base,durable_offset=END+1,normalized_path=[]),dict(base,durable_offset=END+1)]
    for phase in PHASES:
        for offset in (0,1,2,3,4,END):
            for digest in (EMPTY,[11]*32,m["sha256"]):
                for final in (None,base["data"],[0,0]):
                    values.append(dict(base,phase=phase,durable_offset=offset,prefix_sha256=digest,committed_final=final))
    values.extend(dict(base,normalized_path=path) for path in ([],[0],[255],[1]*4104,[2]*4105))
    values.extend(dict(base,created=created,updated=updated,expires=expiry) for created,updated,expiry in (
        (-100,-1,END),(-2**63,0,-2**63),(-20,-20,RETENTION-20),(-20,-20,RETENTION-21),(1,0,RETENTION),
        (-2**63,-2**63,-2**63+RETENTION),(END,END,END),(-2**63,1,-2**63)))
    large=dict(m,source=[8]*4096,destination=[9]*4096)
    values.append(dict(base,manifest=list(manifest["encode"](large)),normalized_path=[7]*4104,
        phase="committed",durable_offset=3,prefix_sha256=m["sha256"],committed_final=base["data"]))
    zero=dict(m,size=0,sha256=EMPTY)
    values.extend([dict(base,manifest=list(manifest["encode"](zero))),
                   dict(base,manifest=list(manifest["encode"](dict(zero,sha256=[77]*32))))])
    sample=encode(base)
    body=sample[:-32]
    wire=[encode(value) for value in values]
    wire.extend(sample[:n] for n in range(len(sample)))
    wire.extend(seal(body[:n]) for n in range(len(body)))
    wire.extend(sample[:n]+bytes([sample[n]^1])+sample[n+1:] for n in range(len(sample)))
    wire.extend([bytes(LIMIT+1),bytes(LIMIT),seal(bytes(LIMIT-32))])
    wire.append(seal(body+b"\0"))
    wire.append(seal(struct.pack(">II",1,LIMIT+1)))
    wire.append(seal(struct.pack(">II",1,LIMIT)))
    wire.append(seal(struct.pack(">I",2)))
    wire.append(seal(struct.pack(">II",1,len(base["manifest"]))+bytes(4)+bytes(base["manifest"])[4:]))
    phase=8+len(base["manifest"])
    for tag in range(256):
        wire.append(seal(body[:phase]+bytes([tag])+body[phase+1:]))
        wire.append(seal(body[:-1]+bytes([tag])))
    return values,list(dict.fromkeys(wire))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","receipt"):
        parser.add_argument(name,type=Path)
    args=parser.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    for checkout,pin in ((original,PIN),(framework,FRAMEWORK)):
        assert run("git","rev-parse","HEAD",cwd=checkout)==pin
        assert not run("git","status","--porcelain",cwd=checkout)
    with tempfile.TemporaryDirectory() as directory:
        work=Path(directory).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=framework)
        cli=work/"tools/debug/oddlang"
        native=work/"native"
        staged_manifest=staged(work,PACKAGE)
        sandbox=project(work,cli,staged_manifest,native,original,CALLER)
        lock=native/"package.lock"
        assert lock.read_bytes()==(PACKAGE.parent/"copy-checkpoint-v1/package.lock").read_bytes()
        old=consumer(original/"crates/marmish-stream",work/"original",original/"Cargo.lock",CALLER)
        new=consumer(native,work/"generated",native/"Cargo.lock",CALLER)
        records,wire=corpus()
        requests=[dict(operation="construct",record=value) for value in records]+[dict(operation="decode",bytes=list(data)) for data in wire]
        expected=[]
        for value in records:
            error=validate(value)
            expected.append(dict(validation=error or {"ok":None},encoding=error or {"ok":list(encode(value))}))
        expected.extend(decode(data) for data in wire)
        for label,exe in (("original",old),("generated",new)):
            for n,(actual,want) in enumerate(zip(observe(exe,requests),expected,strict=True)):
                assert actual==want,(label,n,actual,want)
        mutated=read(staged_manifest)
        pin=mutated["components"][-1]["graph"]
        graph_path=staged_manifest.parent/pin["path"]
        graph=read(graph_path)
        graph["integrity"]["domain"][0]^=1
        alternate=bytes(graph["integrity"]["domain"])
        write(graph_path,graph)
        pin["sha256"]=sha(graph_path)
        write(staged_manifest,mutated)
        mutant=work/"mutant"
        project(work,cli,staged_manifest,mutant,original,CALLER)
        observer=consumer(mutant,work/"mutation",mutant/"Cargo.lock",CALLER)
        results=observe(observer,[requests[0],dict(operation="decode",bytes=list(encode(records[0])))])
        assert results==[dict(validation={"ok":None},encoding={"ok":list(encode(records[0],alternate))}),
                         fault("copy_checkpoint_checksum")]
        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
             originalAndCompanionReadDenied=sandbox,packageLockSha256=sha(lock),callerSha256=sha(CALLER),
             constructions=len(records),decodes=len(wire),rejectedDecodes=sum("error" in row for row in expected[len(records):]),
             observationsSha256=hashlib.sha256(json.dumps(expected,separators=(",",":")).encode()).hexdigest(),
             negativeControl="repinning the integrity domain changes the encoded footer and rejects the original digest"))
        print(f"Qualified {len(records)} native Checkpoint constructions and {len(wire)} binary decodes.")


if __name__=="__main__":
    main()
