"""Black-box integrity block cases. Both peers consume each emitted representation."""
import base64
import copy
import hashlib
import json
import random
import subprocess
import zlib

LIMIT = 8192
MAX_OFFSET = 2**63-1


def send(exe, cases):
    requests = "".join(json.dumps(row, separators=(",", ":"))+"\n" for row in cases.items())
    result = subprocess.run([exe], input=requests, text=True, capture_output=True, check=True)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    if [row[0] for row in rows] != list(cases):
        raise ValueError("peer omitted, reordered or duplicated a result")
    return dict(rows)


def encode_cases():
    rng = random.Random(0x54FE)
    cases = {}
    for size in [0, 1, 2, 3, 4, 17, 31, 127, 1024, LIMIT-1, LIMIT, LIMIT+1]:
        for family, raw in [("run", bytes([129])*size), ("noise",rng.randbytes(size))]:
            for label in ("none","zlib"):
                for offset in (0, MAX_OFFSET-size, MAX_OFFSET-size+1, 2**64-1):
                    cases[f"{family}/{size}/{label}/{offset}"] = dict(
                        operation="encode",offset=offset,bytes=list(raw),negotiated=label)
    return cases


def decode_cases():
    raw = b'blue'*173
    member = zlib.compress(raw)
    def wire(data):
        return dict(offset=7,codec="zlib",raw_len=len(raw),sha256=hashlib.sha256(raw).hexdigest(),
                    data=base64.b64encode(data).decode())
    witnesses = {"valid":wire(member), "concatenated":wire(member+member),
                 "trailing":wire(member+b'\0'), "dictionary":wire(bytes([member[0],member[1]|32])+member[2:])}
    for n in range(len(member)):
        witnesses[f"truncated/{n}"] = wire(member[:n])
    for k,v in [("codec","none"),("raw_len",0),("raw_len",len(member)),("raw_len",8193),
                ("offset",MAX_OFFSET),("sha256","00"*32),("sha256","AA"*32),("sha256",""),
                ("data","Zh=="),("data","A"*10925),("extra",1)]:
        w=wire(member)
        w[k]=v
        witnesses[f"field/{k}/{str(v)[:8]}"]=w
    base=wire(member)
    witnesses["tuple"]=list(base.values())
    witnesses["tuple-extra"]=list(base.values())+[0]
    witnesses["tuple-short"]=list(base.values())[:-1]
    for k in base:
        w=copy.copy(base)
        del w[k]
        witnesses["missing/"+k]=w
    return {f"{k}/{neg}":dict(operation="decode",wire=v,negotiated=neg)
            for k,v in witnesses.items() for neg in ("none","zlib")}


def qualify(reference, candidate):
    declarations={"public":{"operation":"declarations","negotiated":"none"}}
    public=send(reference,declarations)
    assert public==send(candidate,declarations)
    assert public["public"]["limits"]==[8192,8192,32768,262144,MAX_OFFSET,256]
    formats=("default","rightWidth","precision","centerFill","leftWidthAndPrecision")
    samples=public["public"]["formattedInvalid"]
    assert [row["payload"] for row in samples] == ["","sentinel","naïve{input}"]
    for row in samples:
        expected="invalid stream input: "+row["payload"]
        assert all(row[fmt] == expected for fmt in formats), row
    enc=encode_cases()
    ref=send(reference,enc)
    got=send(candidate,enc)
    mixed={}
    for id,request in enc.items():
        original,other=ref[id],got[id]
        raw=bytes(request["bytes"])
        expected_error = ("Limit" if not 1 <= len(raw) <= LIMIT else
                          "Offset" if request["offset"]+len(raw)>MAX_OFFSET else None)
        if expected_error:
            assert original==other and original["error"]==expected_error, id
            continue
        for peer,reply in (("original",original),("generated",other)):
            w=reply["wire"]
            assert (w["offset"],w["raw_len"],w["sha256"]) == (
                request["offset"],len(raw),hashlib.sha256(raw).hexdigest()),id
            data=base64.b64decode(w["data"],validate=True)
            if w["codec"]=="none":
                assert data==raw,id
            else:
                assert request["negotiated"]==w["codec"]=="zlib" and len(data)<len(raw),id
                assert zlib.decompress(data)==raw,id
            mixed[f"{id}/{peer}"]=dict(operation="decode",wire=w,negotiated=request["negotiated"])
        assert original["wire"]["codec"]==other["wire"]["codec"],id
    for exe in (reference,candidate):
        replies=send(exe,mixed)
        for id,val in replies.items():
            inp=enc[id.rsplit("/",1)[0]]
            assert val==dict(decoded={"Ok":inp["bytes"]},valid={"Ok":None},
                             end={"Ok":inp["offset"]+len(inp["bytes"])}),id
    bad=decode_cases()
    refs=send(reference,bad)
    comparison=send(candidate,bad)
    assert refs==comparison, [key for key in refs if refs[key]!=comparison.get(key)]
    return dict(publicErrorVariants=9,publicConstants=6,formattedInvalidCases=len(samples)*len(formats),
                formattedInvalidObservations=samples,encodeCases=len(enc),decodeCases=len(bad),crossReadsOnEachPeer=len(mixed),
                validSeedDigest=hashlib.sha256(b'blue'*173).hexdigest())
