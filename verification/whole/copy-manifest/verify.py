"""Compare the source-free Manifest API with the pinned native and independent oracle."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import PIN, ROOT, consumer, observe, read, run, sha, write
from verify import project, staged

CALLER = HERE / "caller.rs"
MANIFEST = ROOT / "oddlang/portable/stream-copy-manifest-package.json"
FRAMEWORK = "e29873828a015a1af201e2887cae85ff4b2faeb8"
LIMIT = 16_384
MAX_OFFSET = 2**63-1
FLAGS = ("overwrite", "preserve_mode", "preserve_mtime")


def fault(name):
    if name == "Offset":
        return {"error":["Offset", "invalid stream offset"]}
    if name == "Limit":
        return {"error":["Limit", "stream capacity reached"]}
    return {"error":[f'Invalid("{name}")', f"invalid stream input: {name}"]}


def validate(value):
    if value["version"] != 1:
        return "copy_version"
    if any(not 1 <= len(value[key]) <= 4096 or 0 in value[key] for key in ("source","destination")):
        return "copy_path"
    if value["size"] > MAX_OFFSET:
        return "Offset"
    mode, mtime = value["mode"], value["mtime"]
    if (mode is not None) != value["preserve_mode"] or (mtime is not None) != value["preserve_mtime"]:
        return "copy_metadata"
    if (mode is not None and (mode & ~0o777 or mode & 0o600 != 0o600)) or (
        mtime is not None and mtime["nanos"] >= 1_000_000_000
    ):
        return "copy_metadata"
    return None


def namespace(value, swapped=False):
    direction = {"put": 1, "get": 2}[value["direction"]]
    identities = ("server", "client", "transfer") if swapped else ("client", "server", "transfer")
    data = b"marmish-copy-state/1\0" + b"".join(bytes(value[key]) for key in identities) + bytes([direction])
    return ".marmish-copy-v1-" + hashlib.sha256(data).hexdigest()


def encode(value):
    def path(key):
        data = bytes(value[key])
        return struct.pack(">I",len(data)) + data
    def option(data, pack):
        return b"\0" if data is None else b"\1" + pack(data)
    return b"".join((struct.pack(">I",value["version"]),
                     *(bytes(value[key]) for key in ("transfer","client","server")),
                     bytes([{"put":1,"get":2}[value["direction"]]]),path("source"),path("destination"),
                     bytes(value[key] for key in FLAGS),struct.pack(">Q",value["size"]),bytes(value["sha256"]),
                     option(value["mode"],lambda mode:struct.pack(">I",mode)),
                     option(value["mtime"],lambda t:struct.pack(">qI",t["seconds"],t["nanos"]))))


def decode(raw):
    if len(raw) > LIMIT:
        return fault("Limit")
    offset = 0
    def take(length):
        nonlocal offset
        if offset+length > len(raw):
            raise ValueError("copy_truncated")
        data = raw[offset:offset+length]
        offset += length
        return data

    def integer(width, signed=False):
        return int.from_bytes(take(width),"big",signed=signed)

    def path():
        size = integer(4)
        if size > 4096:
            raise ValueError("Limit")
        return list(take(size))

    def tag(error):
        byte = integer(1)
        if byte > 1:
            raise ValueError(error)
        return byte

    try:
        value = {"version":integer(4)}
        value.update({key:list(take(32)) for key in ("transfer","client","server")})
        direction = integer(1)
        if direction not in (1,2):
            raise ValueError("copy_enum")
        value["direction"] = {1:"put",2:"get"}[direction]
        value["source"], value["destination"] = path(), path()
        value.update({key:bool(tag("copy_boolean")) for key in FLAGS})
        value["size"] = integer(8)
        if value["size"] > MAX_OFFSET:
            raise ValueError("Offset")
        value["sha256"] = list(take(32))
        value["mode"] = integer(4) if tag("copy_option") else None
        value["mtime"] = dict(seconds=integer(8,signed=True),nanos=integer(4)) if tag("copy_option") else None
        if offset != len(raw):
            raise ValueError("copy_trailing")
        if error := validate(value):
            raise ValueError(error)
        return {"ok":{"manifest":value,"namespace":namespace(value)}}
    except ValueError as error:
        return fault(str(error))


def corpus():
    rng = random.Random(2741)
    base = dict(version=1, transfer=list(range(32)), client=list(range(32,64)), server=list(range(64,96)),
                direction="put", source=[255,128,1],destination=[254,129], overwrite=True, preserve_mode=True,
                preserve_mtime=True, size=MAX_OFFSET, sha256=[255]*32, mode=0o644,
                mtime=dict(seconds=-4781,nanos=999_999_999))
    constructions = [base, dict(base, direction="get"), dict(base, size=0), dict(base, size=MAX_OFFSET+1),
                     dict(base,size=2**64-1),dict(base,version=0),dict(base,version=2**32-1),
                     dict(base,version=0,source=[],size=MAX_OFFSET+1),dict(base,source=[],size=MAX_OFFSET+1)]
    constructions.extend([
        dict(base,size=MAX_OFFSET+1,mode=0o40),
        dict(base,source=[0],mode=0o40),
        dict(base,destination=[],preserve_mtime=False),
        dict(base,version=0,source=[0]),
    ])
    for mode in range(512):
        constructions.append(dict(base, mode=mode))
    for mode in (512,1024,2**32-1):
        constructions.append(dict(base, mode=mode))
    for mode in (None,0o600,0o40):
        for preserve in (True,False):
            constructions.append(dict(base,mode=mode,preserve_mode=preserve))
    for mtime in (None,*[dict(seconds=s,nanos=n) for s in (-(2**63),-1,0,2**63-1)
                         for n in (0,999_999_999,1_000_000_000,2**32-1)]):
        for preserve in (True,False):
            constructions.append(dict(base,mtime=mtime,preserve_mtime=preserve))
    for key in ("source","destination"):
        for path in ([],[0],[255],[255,0],[255]*4095,[1]*4096,[1]*4097):
            constructions.append(dict(base,**{key:path}))
    constructions.append(dict(base,source=[254]*4096,destination=[255]*4096))
    for i in range(48):
        keys = ("transfer","client","server","sha256")
        fields = {key:list(rng.randbytes(32)) for key in keys}
        constructions.append(dict(base,**fields,direction=("put","get")[i%2],overwrite=bool(i%2),
                                  preserve_mtime=bool(i%2),mtime=(base["mtime"] if i%2 else None),
                                  preserve_mode=bool(i%3),mode=(0o700 if i%3 else None)))

    sample = encode(base)
    wire = [encode(value) for value in constructions]
    wire.extend(sample[:size] for size in range(len(sample)))
    wire.extend([sample+b"\0",sample+b"x"*(LIMIT-len(sample)),sample+b"x"*(LIMIT+1-len(sample))])
    source = 4+3*32+1
    destination = source+4+len(base["source"])
    bools = destination+4+len(base["destination"])
    size = bools+3
    mode = size+8+32
    mtime = mode+5
    def replace(data, at, new):
        return data[:at]+new+data[at+len(new):]
    for at in (source,destination):
        for count in (0,1,4096,4097,2**32-1):
            wire.append(replace(sample,at,struct.pack(">I",count)))
            wire.append(replace(sample[:at+4],at,struct.pack(">I",count)))
    for at in (source-1,bools,bools+1,bools+2,mode,mtime):
        for byte in range(256):
            wire.append(replace(sample,at,bytes([byte])))
    offset_fault = replace(sample,size,struct.pack(">Q",MAX_OFFSET+1))
    wire.extend([offset_fault[:size+8],replace(offset_fault,mode,b"\xff"),offset_fault+b"\xff"])
    for broken in (dict(base,version=0),dict(base,source=[0]),dict(base,destination=[]),
                   dict(base,mode=0o40),dict(base,mtime=dict(seconds=0,nanos=1_000_000_000))):
        wire.append(encode(broken)+b"\0")
    return constructions,list(dict.fromkeys(wire))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ("original","framework","receipt"):
        parser.add_argument(argument,type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    for checkout,pin in ((original,PIN),(framework,FRAMEWORK)):
        assert run("git","rev-parse","HEAD",cwd=checkout) == pin
        assert not run("git","status","--porcelain",cwd=checkout)

    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=framework)
        cli = work/"tools/debug/oddlang"
        manifest = staged(work,MANIFEST)
        dest = work/"native"
        sandbox = project(work,cli,manifest,dest,original,CALLER)
        lock = dest/"package.lock"
        assert (MANIFEST.parent/"copy-manifest-v1/package.lock").read_bytes() == lock.read_bytes()
        old = consumer(original/"crates/marmish-stream",work/"old",original/"Cargo.lock",CALLER)
        new = consumer(dest,work/"new",dest/"Cargo.lock",CALLER)
        values, wire = corpus()
        requests = [dict(operation="construct",manifest=value) for value in values]
        requests += [dict(operation="decode",bytes=list(data)) for data in wire]
        expected = []
        for value in values:
            error = validate(value)
            expected.append(dict(validation=fault(error) if error else {"ok":None},
                                 encode=fault(error) if error else {"ok":list(encode(value))},
                                 namespace=namespace(value)))
        expected += [decode(data) for data in wire]
        digests = [hashlib.sha256(encode(value)).hexdigest() for value in values if not validate(value)]
        for label,observations in (("original",observe(old,requests)),("generated",observe(new,requests))):
            assert len(observations)==len(expected)
            for index,(got,want) in enumerate(zip(observations,expected,strict=True)):
                assert got==want,(label,index,got,want)
            assert [hashlib.sha256(bytes(row["encode"]["ok"])).hexdigest() for row in observations[:len(values)]
                    if "ok" in row["encode"]] == digests

        mutated = read(manifest)
        pin = mutated["components"][-1]["graph"]
        graph_path = manifest.parent/pin["path"]
        graph = read(graph_path)
        views = list(graph["hashViews"].values())
        fields = views[0]["fields"]
        fields[0],fields[1] = fields[1],fields[0]
        size_slot, = [slot for slot in graph["layout"] if slot["id"].endswith(":field:size")]
        del size_slot["encoding"]["readError"]
        write(graph_path,graph)
        pin["sha256"] = sha(graph_path)
        write(manifest,mutated)
        mutant = work/"mutant"
        project(work,cli,manifest,mutant,original,CALLER)
        probe = consumer(mutant,work/"probe",mutant/"Cargo.lock",CALLER)
        changed, = observe(probe,requests[:1])
        assert changed == dict(expected[0],namespace=namespace(values[0],swapped=True))
        assert changed != expected[0]
        size_end = 4+3*32+1+8+len(values[0]["source"])+len(values[0]["destination"])+3+8
        short = encode(dict(values[0],size=MAX_OFFSET+1))[:size_end]
        assert decode(short) == fault("Offset")
        assert observe(probe,[dict(operation="decode",bytes=list(short))]) == [fault("copy_truncated")]
        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
             originalAndCompanionReadDenied=sandbox,packageLockSha256=sha(lock),callerSha256=sha(CALLER),
             constructions=len(values),decodes=len(wire),rejectedDecodes=sum("error" in row for row in expected[len(values):]),
             encodedDigests=len(digests),digestListSha256=hashlib.sha256("".join(digests).encode()).hexdigest(),
             observationsSha256=hashlib.sha256(json.dumps(expected,separators=(",",":")).encode()).hexdigest(),
             negativeControl="repinned namespace order changes exactly the predicted digest; deferring the size range makes truncation win over Offset"))
        print(f"Qualified {len(values)} Manifest constructions, {len(wire)} decodes and {len(digests)} canonical digests.")


if __name__ == "__main__":
    main()
