"""Verify offline vault public values through original and generated native APIs."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/"copy-launch"))
from inputs import PIN, ROOT, archive_at, linux_project, read, run, sha, staged, write

PORTABLE=ROOT/"oddlang/portable"
DATA=PORTABLE/"vault-values-v1"
PACKAGE=PORTABLE/"vault-value-domain-package.json"
CRATE=Path("crates/marmish-vault")
SOURCE=CRATE/"src/lib.rs"
CALLER=HERE/"caller.rs"
SCOPE={"Namespace","Purpose","KeyRef","Label","Metadata","Error"}
FAULT="Malformed/malformed/malformed"


def checked(*command,cwd=None,env=None):
    return subprocess.run(command,cwd=cwd,env=env,check=True)


def shard(framework,manifest,target):
    evidence=subprocess.check_output(["bash",framework/"verification/rust-recovery/run.sh",
                          manifest,"marmish-vault","lib","marmish_vault",target,"default"],
                          env=dict(os.environ,ODD_RECOVERY_DATA_SHAPES="1"))
    data=json.loads(evidence)
    assert data["crateName"]=="marmish_vault"
    def native_type(value):
        if value["kind"]=="unsigned-integer":
            return "u"+str(value["bits"])
        assert value["kind"]=="named" and value["arguments"]==[],value
        return value["id"].removesuffix("::{type}").rsplit("::",1)[-1]
    selected={}
    for declaration in data["graph"]["declarations"]:
        name=declaration["name"]
        if name not in SCOPE or "::helper::" in declaration["id"]:
            continue
        assert name not in selected and declaration["parameters"]==[],declaration
        if declaration["kind"]=="sum":
            variants=declaration["variants"]
            assert all(case["form"]=="unit" and case["fields"]==[] for case in variants)
            selected[name]={"kind":"sum","orderedVariants":[case["name"] for case in
                            sorted(variants,key=lambda case:case["position"])]}
        else:
            assert declaration["kind"]=="product",declaration
            selected[name]={"kind":"product","form":declaration["form"],"orderedFields":[
                dict(name=field["name"],type=native_type(field["type"]))
                for field in sorted(declaration["fields"],key=lambda field:field["position"])]}
    assert set(selected)==SCOPE
    return selected


def expected(maximum):
    enum_matches=["Legacy/1","MarmishV2/2"]
    purpose_matches=["Nostr/nostr","Sqlcipher/sqlcipher"]+[FAULT]*6
    named_errors=[f"{name}/{category}/{category}" for name,category in (
      ("Unavailable","unavailable"),("Uninitialized","uninitialized"),
      ("LockedOrDenied","locked_or_denied"),("Missing","missing"),("Exists","exists"),
      ("Ambiguous","ambiguous"),("Malformed","malformed"),("BackendFailure","backend_failure"))]
    def verdict(allowed):
        return "ok" if allowed else FAULT
    references=[verdict(version in (1,2)) for version in range(256)]
    references += [verdict(length==64) for length in (0,1,63,64,65)]
    references += [verdict(chr(cp) in "0123456789abcdef") for cp in range(128)] + [FAULT,FAULT]
    labels=["a"*n for n in (0,1,126,127,128,129)]
    labels.extend(["é"*64,"🦀"*32,"a"*125+"🦀","a"*126+"é","a"*127+"é",
                   "a"*128+chr(31),chr(0x200b),chr(0xfeff)])
    labels.extend("A"+chr(n)+"B" for n in (*range(0xa1),*range(0x2027,0x2030),*range(0x2065,0x206b)))
    def allowed(text):
        excluded=lambda n: (n<=31 or 127<=n<=159 or 0x2028<=n<=0x202e or 0x2066<=n<=0x2069)
        return 0<len(text.encode("utf-8"))<=maximum and not any(excluded(ord(ch)) for ch in text)
    debug='Metadata { key_ref: KeyRef { version: 2, purpose: Sqlcipher, id: "'+"a"*64+'" }, label: Label("local-index") }'
    return enum_matches+purpose_matches+named_errors+references+[verdict(allowed(text)) for text in labels]+[debug]


def mutate(original, path):
    archive_at(original,PIN,path)
    source=path/SOURCE
    before=source.read_text()
    original_bound="|| value.len() > 128"
    assert before.count(original_bound)==1
    source.write_text(before.replace(original_bound,"|| value.len() > 127"))


def isolated(work,cli,package,target,original,mutant):
    assert not list(package.parent.rglob("*.rs"))
    if platform.system()=="Linux":
        return linux_project(cli,package,target,original,CALLER,extra_sources=(mutant,))
    assert platform.system()=="Darwin"
    private=(ROOT,original,mutant,work/"framework")
    profile="(version 1)(allow default)(deny network*)"+"".join(
        "(deny file-read* (subpath "+json.dumps(str(path))+"))" for path in private)
    wrapper=["/usr/bin/sandbox-exec","-p",profile]
    for protected in (original/SOURCE,mutant/SOURCE,CALLER,DATA/"shapes.json",work/"framework/Cargo.toml"):
        result=subprocess.run([*wrapper,"/bin/cat",protected],capture_output=True)
        assert result.returncode and b"Operation not permitted" in result.stderr,protected
    checked(*wrapper,cli,"build","native-package","rust",package,target,cwd=work)
    checked(*wrapper,"cargo","+1.97.1","build","--locked","--offline","--release",cwd=target)
    return True


def consumer(package,work,lock,privacy=False):
    (work/"src").mkdir(parents=True)
    shutil.copyfile(CALLER,work/"src/main.rs")
    (work/"Cargo.toml").write_text(
        '[package]\nname="observe-vault-values"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nmarmish-vault={path='+json.dumps(str(package.resolve()))+'}\n[workspace]\n')
    shutil.copyfile(lock,work/"Cargo.lock")
    metadata=json.loads(run("cargo","+1.97.1","metadata","--offline","--format-version","1",cwd=work))
    native_package,=[item for item in metadata["packages"] if item["name"]=="marmish-vault"]
    assert (native_package["version"],native_package["edition"])==("0.1.0","2021")
    checked("cargo","+1.97.1","build","--release","--offline","--locked",
            "--target-dir","target",cwd=work)
    lines=run(work/"target/release/observe-vault-values").splitlines()
    if privacy:
        for type_name, expression, code in (
            ("KeyRef", 'KeyRef { version: 2, purpose: Purpose::Nostr, id: "arbitrary".into() }', "E0451"),
            ("Label", 'Label("arbitrary".into())', "E0423"),
        ):
            (work/"src/main.rs").write_text(
                f"use marmish_vault::{{KeyRef, Label, Purpose}};\nfn main() {{ let _ = {expression}; }}\n"
            )
            probe=subprocess.run(["cargo","+1.97.1","check","--release","--offline","--locked"],
                                 cwd=work,capture_output=True,text=True)
            assert probe.returncode and "private" in probe.stderr and type_name in probe.stderr and code in probe.stderr,probe.stderr
    return lines


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","receipt"):
        parser.add_argument(name,type=Path)
    args=parser.parse_args()
    original,framework=args.original.resolve(),args.framework.resolve()
    pin=read(HERE/"reference-aarch64-apple-darwin.json")["framework"]
    assert run("git","rev-parse","HEAD",cwd=framework)==pin
    assert run("git","rev-parse","HEAD",cwd=original)==PIN
    assert not run("git","status","--porcelain",cwd=original)
    provenance=read(DATA/"provenance.json")
    assert sha(original/SOURCE)==provenance["sourceSha256"]
    rust=run("rustc","+1.97.1","-vV")
    host=next(line.split()[1] for line in rust.splitlines() if line.startswith("host:"))
    with tempfile.TemporaryDirectory(prefix="vault-values-") as d:
        work=Path(d).resolve()
        fw,tools,mutant=(work/name for name in ("framework","tools","mutated-source"))
        archive_at(framework,pin,fw)
        checked("cargo","+1.97.1","build","--locked","--offline","--bin","oddlang","--target-dir",tools,cwd=fw)
        cli=tools/"debug/oddlang"
        mutate(original,mutant)
        package=staged(work,PACKAGE)
        native,changed=work/"native",work/"changed"
        assert isolated(work,cli,package,native,original,mutant)
        assert (native/"package.lock").read_bytes()==(DATA/"package.lock").read_bytes()
        assert shard(fw,original/CRATE/"Cargo.toml",host)==read(DATA/"shapes.json")
        assert shard(fw,native/"Cargo.toml",host)==read(DATA/"shapes.json")
        spec=read(package)
        component,=[item for item in spec["components"] if item["kind"]=="checked-values"]
        graph=package.parent/component["graph"]["path"]
        before=sha(graph)
        semantic=read(graph)
        rule,=[r for r in semantic["types"]["urn:marmish:vault:label"]["guards"] if r["kind"]=="utf8-byte-length"]
        assert rule["maximum"]==128
        rule["maximum"]=127
        write(graph,semantic)
        component["graph"]["sha256"]=sha(graph)
        write(package,spec)
        assert isolated(work,cli,package,changed,original,mutant)
        baseline,mutated=expected(128),expected(127)
        cases=(
            ("upstream",original/CRATE,original/"Cargo.lock",baseline,True),
            ("upstream-mutated",mutant/CRATE,mutant/"Cargo.lock",mutated,False),
            ("generated",native,native/"Cargo.lock",baseline,True),
            ("generated-mutated",changed,changed/"Cargo.lock",mutated,False),
        )
        for name,crate,lock,expected_lines,privacy in cases:
            got=consumer(crate,work/name,lock,privacy)
            assert got==expected_lines,name
            if name=="upstream-mutated":
                assert got!=baseline
        write(args.receipt,dict(original=PIN,framework=pin,rust=rust,declaredShapes=sorted(SCOPE),
              observations=4*len(baseline),changedOutcomes=sum(x!=y for x,y in zip(baseline,mutated,strict=True)),
              originalSourceSha256=provenance["sourceSha256"],mutatedSourceSha256=sha(mutant/SOURCE),
              originalGraphSha256=before,mutatedGraphSha256=sha(graph),
              originalPackageLockSha256=sha(native/"package.lock"),mutatedPackageLockSha256=sha(changed/"package.lock"),
              callerSha256=sha(CALLER),shapesSha256=sha(DATA/"shapes.json"),sourceReadDenied=True,
              privateStorageRejectsExternalConstruction=True))
        print("Qualified native vault values and a UTF-8 byte limit mutation without platform access.")


if __name__=="__main__":
    main()
