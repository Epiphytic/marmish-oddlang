"""Verify pinned fragment calls and two upstream writer-order mutations."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent/"copy-launch"))
from inputs import PIN, ROOT, read, run, sha, write
from verify import staged

PREVIOUS = runpy.run_path(str(HERE.parent/"source-copy-frame/verify.py"))
archive_at = PREVIOUS["archive_at"]
checked = PREVIOUS["checked"]
sealed = PREVIOUS["sealed"]
PORTABLE = ROOT/"oddlang/portable"
DATA = PORTABLE/"source-copy-frame-fragments-v1"
PACKAGE = PORTABLE/"stream-source-copy-frame-fragments-package.json"
ROLE = {"launch":"Start", "manifest":"Manifest"}
BINDING = {
    "launch": PORTABLE/"copy-frame-v1/launch-fragment-rust.json",
    "manifest": PORTABLE/"copy-manifest-v1/rust.json",
}
SOURCE = Path("crates/marmish-stream/src/copy_protocol.rs")
CALLER = HERE/"caller.rs"
METHOD = "crate::copy_protocol::Frame::encode"


def derive(framework, cli, checkout, host, dest):
    dest.mkdir()
    evidence = dest/"evidence.json"
    crate = checkout/SOURCE.parents[1]
    source = checkout/SOURCE
    with evidence.open("w") as stream:
        subprocess.run(["bash", framework/"verification/rust-recovery/run.sh",
                        crate/"Cargo.toml", "marmish-stream", "lib", "marmish_stream", host, "default"],
                       stdout=stream, check=True,
                       env=dict(os.environ, ODD_RECOVERY_TAGGED_FRAME_FRAGMENTS=METHOD+"#Start,Manifest"))
    for name, variant in ROLE.items():
        selection = dest/(name+"-selection.json")
        pin = read(DATA/selection.name)
        assert pin["variant"] == variant
        pin.update(sourceSha256=sha(source), evidenceSha256=sha(evidence))
        write(selection,pin)
        checked(cli,"recover","frame-fragment",crate,source,evidence,selection,
                DATA/(name+"-template.json"),BINDING[name],dest/(name+"-recovery"))
        for output, suffix in (("graph.json",".json"),("dependencies.json","-dependencies.json")):
            (dest/(name+"-recovery")/output).rename(dest/(name+suffix))


def swap_fields(original, mutant):
    archive_at(original,PIN,mutant)
    source=mutant/SOURCE
    text=source.read_text()
    before = "    out.extend(v.client);\n    out.extend(v.server);"
    after = "    out.extend(v.server);\n    out.extend(v.client);"
    assert text.count(before) == len(ROLE)
    for name in ROLE:
        start = text.index(f"fn encode_{name}(")
        stop = text.index("\n}",start)
        assert text[start:stop].count(before) == 1
        text = text[:start] + text[start:stop].replace(before,after) + text[stop:]
    source.write_text(text)


def expected(altered):
    def u32(n): return n.to_bytes(4,"big")
    def envelope(tag, payload): return (bytes([tag])+u32(len(payload))+payload).hex()
    owners = bytes([0x12])*32 + (bytes([0x56])*32+bytes([0x34])*32 if altered else
                                bytes([0x34])*32+bytes([0x56])*32)
    fields = owners + b"\x01" + u32(2)+b"\xab\xcd" + u32(1)+b"\xe1"
    start = u32(1)+b"\x01"+fields+b"\x01\x00\x00"
    end = (0x0213).to_bytes(8,"big")+bytes([0x78])*32
    manifest = u32(1)+fields
    empty = manifest+b"\x01\x00\x00"+end+b"\x00\x00"
    filled = (manifest+b"\x01\x01\x01"+end+b"\x01"+u32(0o640)+
              b"\x01"+(-3).to_bytes(8,"big",signed=True)+u32(9))
    invalid = lambda name: f'Invalid("copy_{name}")'
    return [envelope(1,start+b"\x00"),envelope(1,start+b"\x01"),invalid("version"),
            invalid("path"),envelope(2,empty),envelope(2,filled),invalid("version"),
            invalid("path"),invalid("metadata")]


def execute(crate,dest,lock):
    (dest/"src").mkdir(parents=True)
    shutil.copyfile(CALLER,dest/"src/main.rs")
    (dest/"Cargo.toml").write_text(
        '[package]\nname="observe-copy-fragments"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nmarmish-stream={path='+json.dumps(str(crate.resolve()))+'}\n[workspace]\n')
    shutil.copyfile(lock,dest/"Cargo.lock")
    run("cargo","+1.97.1","metadata","--offline","--format-version","1",cwd=dest)
    checked("cargo","+1.97.1","build","--release","--offline","--locked",cwd=dest)
    return run(dest/"target/release/observe-copy-fragments").splitlines()


def check_fresh_release(original_recovery, mutant_recovery):
    for name in ["evidence.json",*(entry+suffix for entry in ROLE for suffix in (".json","-selection.json","-dependencies.json"))]:
        assert (original_recovery/name).read_bytes()==(DATA/name).read_bytes(),name
    for name in ROLE:
        graph=read(PORTABLE/"copy-frame-v1"/(name+"-fragment.json"))
        baseline=read(original_recovery/(name+".json"))
        assert baseline==graph,name
        array=graph["layout"]
        client=next(i for i,slot in enumerate(array) if slot["id"].endswith(":client"))
        server=next(i for i,slot in enumerate(array) if slot["id"].endswith(":server"))
        array[client],array[server]=array[server],array[client]
        assert read(mutant_recovery/(name+".json"))==graph,name
        assert (mutant_recovery/(name+"-dependencies.json")).read_bytes()==(DATA/(name+"-dependencies.json")).read_bytes()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for argument in ("original","framework","receipt"):
        parser.add_argument(argument,type=Path)
    args=parser.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    pin=read(HERE/"reference-aarch64-apple-darwin.json")["framework"]
    assert run("git","rev-parse","HEAD",cwd=framework)==pin
    assert run("git","rev-parse","HEAD",cwd=original)==PIN
    assert not run("git","status","--porcelain",cwd=original)
    rust=run("rustc","+1.97.1","-vV")
    host=next(line.split()[1] for line in rust.splitlines() if line.startswith("host:"))
    with tempfile.TemporaryDirectory(prefix="copy-fragment-source-") as d:
        work=Path(d).resolve()
        fw,tools,mutant=(work/name for name in ("framework","tools","mutant-reference"))
        archive_at(framework,pin,fw)
        checked("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",tools,cwd=fw)
        cli=tools/"debug/oddlang"
        swap_fields(original,mutant)
        baseline,changed=work/"baseline-recovery",work/"mutant-recovery"
        derive(fw,cli,original,host,baseline)
        derive(fw,cli,mutant,host,changed)
        check_fresh_release(baseline,changed)
        package=staged(work,PACKAGE)
        old,new=work/"baseline-package",work/"changed-package"
        assert sealed(work,cli,package,old,original,mutant)
        assert (old/"package.lock").read_bytes()==(DATA/"package.lock").read_bytes()
        assembly=read(package)
        for name in ROLE:
            path=f"source-copy-frame-fragments-v1/{name}.json"
            part,=[part for part in assembly["components"] if part["graph"]["path"]==path]
            (package.parent/path).write_bytes((changed/(name+".json")).read_bytes())
            part["graph"]["sha256"]=sha(changed/(name+".json"))
        write(package,assembly)
        assert sealed(work,cli,package,new,original,mutant)
        for tag,crate,lock,altered in (
            ("reference",original/SOURCE.parents[1],original/"Cargo.lock",False),
            ("reference-mutated",mutant/SOURCE.parents[1],mutant/"Cargo.lock",True),
            ("generated",old,old/"Cargo.lock",False),
            ("generated-mutated",new,new/"Cargo.lock",True),
        ):
            assert execute(crate,work/tag,lock)==expected(altered),tag
        evidence=read(baseline/"evidence.json")
        write(args.receipt,dict(original=PIN,framework=pin,rust=rust,variants=len(ROLE),
              slots=sum(len(case["slots"]) for case in evidence["cases"].values()),
              observations=4*len(expected(False)),
              changedOutcomes=sum(a!=b for a,b in zip(expected(False),expected(True),strict=True)),
              originalEvidenceSha256=sha(baseline/"evidence.json"),mutatedEvidenceSha256=sha(changed/"evidence.json"),
              originalGraphs={name:sha(baseline/(name+".json")) for name in ROLE},
              mutatedGraphs={name:sha(changed/(name+".json")) for name in ROLE},
              originalPackageLockSha256=sha(old/"package.lock"),mutatedPackageLockSha256=sha(new/"package.lock"),
              callerSha256=sha(CALLER),sourceReadDenied=True))
        print("Qualified checked Start and Manifest fragment delegation and both source mutations.")


if __name__=="__main__":
    main()
