"""Reproduce the portable Launch release, compare both native APIs, then falsify its guard order."""
import argparse
import base64
from hashlib import sha256
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

from inputs import DATA, HERE, PIN, ROOT, consumer, derive, observe, read, run, sha, write

FRAMEWORK = "47958722555a16892e47985eef85de55a9c057f5"
MANIFEST = DATA.parent / "stream-launch-package.json"


def compact(value):
    return json.dumps(value, separators=(",", ":"))


def canonical(req):
    source, destination = bytes(req["source"]), bytes(req["destination"])
    version = req["version"]
    error = None
    if version != 1:
        error = "copy_version"
    elif any(not 1 <= len(path) <= 4096 or 0 in path for path in (source, destination)):
        error = "copy_path"
    payload = dict(version=version, mode=req["mode"], transfer="01"*32, client="02"*32,
                   server="03"*32, direction=req["direction"],
                   source=base64.b64encode(source).decode(), destination=base64.b64encode(destination).decode(),
                   overwrite=True, preserve_mode=False, preserve_mtime=True)
    return {"wire": compact(payload), "valid": {"Err": [f'Invalid("{error}")', f"invalid stream input: {error}"]} if error else {"Ok": None}}


def corpus():
    base = dict(operation="construct", version=1, mode="create", direction="put", source=[1], destination=[255])
    creates = [dict(base, version=v, mode=m, direction=d) for v in (0,1,2,2**32-1)
               for m in ("create", "resume") for d in ("put", "get")]
    for role in ("source", "destination"):
        for path in ([], [0], [0,2], [1,0], [254]*4095, [255]*4096, [255]*4097):
            for version in (0, 1):
                creates.append(dict(base, version=version, **{role:path}))
    creates.append(dict(base, source=[], destination=[]))
    wire = json.loads(canonical(base)["wire"])
    texts = [compact(wire), compact(dict(reversed(list(wire.items())))), compact(list(wire.values())),
             compact(list(wire.values())[:-1]), compact(list(wire.values())+[True]),
             '{"version":1,'+compact(wire)[1:], compact(dict(wire, extraneous=True)), "null", "{}"]
    for field in wire:
        texts.append(compact({k:v for k,v in wire.items() if k != field}))
    for field in ("transfer", "client", "server"):
        for value in ("AA"*32, "xx"*32, "00"*31, "a0"*32, None):
            texts.append(compact(dict(wire, **{field:value})))
    for field in ("source", "destination"):
        for value in ("", "AA==", "/w==", "AB==", "?", "?"*5465,
                      base64.b64encode(b"b"*4096).decode(), base64.b64encode(b"b"*4097).decode()):
            texts.append(compact(dict(wire, **{field:value})))
    for field,value in (("mode","future"), ("direction","future"), ("version",-1),
                        ("version",2**32), ("overwrite","true")):
        texts.append(compact(dict(wire, **{field:value})))
    return creates, [dict(operation="decode",text=text) for text in texts]


def staged(work):
    stage = work / "inputs"
    stage.mkdir()
    manifest = read(MANIFEST)
    pins = [manifest["cargoLock"]] + [part[k] for part in manifest["components"] for k in ("graph","overlay")]
    for pin in pins:
        source = MANIFEST.parent / pin["path"]
        assert sha(source) == pin["sha256"]
        dest = stage / pin["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
    shutil.copyfile(MANIFEST, stage/MANIFEST.name)
    assert not list(stage.rglob("*.rs"))
    return stage / MANIFEST.name


def project(work, cli, manifest, dest, original):
    wrapper = []
    if platform.system() == "Darwin":
        profile = '(version 1)(allow default)(deny network*)' + ''.join(
            '(deny file-read* (subpath '+json.dumps(str(p.resolve()))+'))' for p in (ROOT,original))
        wrapper = ["/usr/bin/sandbox-exec", "-p", profile]
        for inaccessible in [ROOT/"verification/whole/copy-launch/caller.rs",original/"Cargo.toml"]:
            control = subprocess.run([*wrapper,"/bin/cat",inaccessible],cwd=work,capture_output=True)
            assert control.returncode and b"Operation not permitted" in control.stderr
    run(*wrapper, cli, "build","native-package","rust",manifest,dest,cwd=work)
    run(*wrapper,"cargo","+1.97.1","build","--locked","--offline","--release",cwd=dest)
    return bool(wrapper)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","receipt"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    original, fw = args.original.resolve(), args.framework.resolve()
    assert run("git","rev-parse","HEAD",cwd=fw) == FRAMEWORK
    assert not run("git","diff","HEAD","--",cwd=fw)
    with tempfile.TemporaryDirectory() as tmp:
        work=Path(tmp).resolve()
        run("cargo","build","--locked","--offline","--bin","oddlang","--target-dir",work/"tools",cwd=fw)
        cli=work/"tools/debug/oddlang"
        manifest=staged(work)
        dest=work/"generated"
        sandbox=project(work,cli,manifest,dest,original)
        assert (dest/"package.lock").read_bytes() == (DATA/"package.lock").read_bytes()
        old=consumer(original/"crates/marmish-stream",work/"reference",original/"Cargo.lock")
        new=consumer(dest,work/"native",dest/"Cargo.lock")
        table=derive(original,cli,old,work/"recovery")
        for name in ("launch.json","launch-rust.json","enums.json","enums-rust.json"):
            assert (work/"recovery"/name).read_bytes()==(DATA/name).read_bytes(),name
        assert (work/"recovery/candidate/observation.json").read_bytes()==(DATA/"observation.json").read_bytes()
        assert observe(new,[dict(operation="enums")])==[table]
        creates,decodes=corpus()
        expected=[canonical(req) for req in creates]
        assert observe(old,creates)==expected==observe(new,creates)
        decoded=observe(old,decodes)
        assert decoded==observe(new,decodes)
        graph_path=manifest.parent/"copy-launch-v1/launch.json"
        graph=read(graph_path)
        graph["guards"].append(graph["guards"].pop(0))
        write(graph_path,graph)
        mutated=read(manifest)
        mutated["components"][-1]["graph"]["sha256"]=sha(graph_path)
        write(manifest,mutated)
        mutant=work/"mutant"
        project(work,cli,manifest,mutant,original)
        negative=consumer(mutant,work/"mutant-observer",mutant/"Cargo.lock")
        req=next(r for r in creates if r["version"]==0 and not r["source"])
        got,=observe(negative,[req])
        assert got["valid"]["Err"][0]=='Invalid("copy_path")' and canonical(req)["valid"]["Err"][0]=='Invalid("copy_version")'
        write(args.receipt,dict(original=PIN,framework=FRAMEWORK,rust=run("rustc","+1.97.1","-vV"),
              originalAndCompanionReadDenied=sandbox,packageLockSha256=sha(DATA/"package.lock"),
              callerSha256=sha(HERE/"caller.rs"),nativeEnumByteOutcomes=5*256,constructCases=len(creates),
              exactDecodeCases=len(decodes),negativeControl="version guard moved last selects copy_path instead of copy_version",
              observationsSha256=sha256(compact([table,expected,decoded]).encode()).hexdigest()))
        print("Qualified", len(creates), "native constructions,", len(decodes), "decodes, and all 1,280 enum inputs.")
if __name__ == "__main__":
    main()
