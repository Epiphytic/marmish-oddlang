"""Combine a pinned structural candidate, a native declaration observer, and reviewed Launch guards."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
DATA=ROOT/"oddlang/portable/copy-launch-v1"
PIN="af54339d250290490d62d4475a77539b7e739071"
LIMITS="urn:marmish:copy:"
LAUNCH=LIMITS+"launch"


def write(path,obj):
    path.write_text(json.dumps(obj,indent=2,ensure_ascii=False)+"\n")
def read(path):
    return json.loads(path.read_bytes())
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def run(*args,cwd=None):
    return subprocess.run(args,cwd=cwd,capture_output=True,text=True,check=True).stdout.strip()
def observe(exe,requests):
    p=subprocess.run([exe],capture_output=True,input="".join(json.dumps(r)+"\n" for r in requests),text=True,check=True)
    rows=[json.loads(line) for line in p.stdout.splitlines()]
    assert len(rows)==len(requests)
    return rows


def consumer(crate,root,lock,caller=HERE/"caller.rs"):
    (root/"src").mkdir(parents=True)
    shutil.copyfile(caller,root/"src/main.rs")
    (root/"Cargo.toml").write_text('[package]\nname="observe-copy-launch"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nserde={version="=1.0.229",features=["derive"]}\nserde_json="=1.0.151"\nmarmish-stream={path='
        + json.dumps(str(crate.resolve()))+'}\n[workspace]\n')
    shutil.copyfile(lock,root/"Cargo.lock")
    run("cargo","+1.97.1","metadata","--offline","--format-version","1",cwd=root)
    run("cargo","+1.97.1","build","--release","--offline","--locked",cwd=root)
    return root/"target/release/observe-copy-launch"


def derive(original,cli,exe,out):
    assert run("git","rev-parse","HEAD",cwd=original)==PIN and not run("git","status","--porcelain",cwd=original)
    out.mkdir()
    run(cli,"recover","wire-schema",original/"crates/marmish-stream/src/copy_protocol.rs",DATA/"selection.json",out/"candidate")
    graph=read(out/"candidate/graph.json")
    binding=read(out/"candidate/rust.json")
    unresolved=graph.pop("unresolved")
    assert sorted(unresolved)==["source item has unverified native derive Default","unexpanded source item macro numbered_enum"]
    record,=[d for d in graph["types"] if d["id"]==LAUNCH]
    # Independently compiled data-shape observer records the macro-expanded field set and order.
    base=ROOT/"oddlang/portable/workspace-shapes-v1"
    lock=read(base/"locks/aarch64-apple-darwin.marmish-stream.lib.marmish_stream.json")
    assert lock["pin"]["sourceRevision"]==PIN
    shape=base/"graphs"/(lock["shapesSha256"]+".json")
    assert sha(shape)==lock["shapesSha256"]
    native,=[d for d in read(shape)["declarations"] if d["name"]=="Launch"]
    assert [f["name"] for f in native["fields"]]==[f["name"] for f in record["fields"]]
    table,=observe(exe,[dict(operation="enums")])
    version=table["version"]
    def guard(label,field,require,error):
        graph["guards"].append({"id":LAUNCH+":guard:"+label,"owner":LAUNCH,
              "require":{"field":LAUNCH+":field:"+field,**require},"error":LIMITS+error})
    guard("version","version",dict(kind="unsigned-range",minimum=version,maximum=version),"copy_version")
    for f in ("source","destination"):
        guard(f+"-length",f,dict(kind="count",minimum=1,maximum=4096),"copy_path")
        guard(f+"-content",f,{"kind":"excludes-byte","byte":0},"copy_path")
    binding["nativeValidation"]={LAUNCH:dict(method="validate",typePath="crate::Error",
           errors={LIMITS+s:dict(variant="Invalid",argument=s) for s in ("copy_version","copy_path")})}
    write(out/"launch.json",graph);write(out/"launch-rust.json",binding)
    text={"format":"oddlang-shared-declarations-v1","id":LIMITS+"numeric-constants-and-enums",
          "constants":[{"id":LIMITS+"VERSION","value":version}],"stringEnums":[]}
    names={LIMITS+"VERSION":"VERSION"};orders={}
    shared=ROOT/"oddlang/portable/stream-shared-v1"
    error,=read(shared/"graph.json")["errors"]
    shared_binding=read(shared/"rust.json")
    error_id=error["id"]
    text["errors"]=[error]
    names.update({i:shared_binding["names"][i] for i in [error_id,*[v["id"] for v in error["variants"]]]})
    orders[error_id]=shared_binding["variantOrder"][error_id]
    invalid,=[v["id"] for v in error["variants"] if names[v["id"]]=="Invalid"]
    for name,rows in table["enums"].items():
        id=LIMITS+name
        names[id]=name
        variants=[];tags={};order=[]
        rejected=rows["reject"]
        assert all(row[1:]==['Invalid("copy_enum")','invalid stream input: copy_enum'] for row in rejected)
        assert len(rows["accept"])+len(rejected)==256
        for v in rows["accept"]:
            vi=id+":"+v["rust"];names[vi]=v["rust"];order.append(vi);tags[vi]=v["tag"]
            variants.append(dict(id=vi,wire=v["wire"]))
        # Preserve the native declaration order using the independent compiler inventory.
        # Numeric iteration alone would silently reorder nonmonotonic discriminants.
        enum,=[d for d in read(shape)["declarations"] if d["name"]==name and "::copy_protocol::" in d["id"] and d["kind"]=="sum"]
        order=[id+":"+v["name"] for v in enum["variants"]]
        assert set(order)==set(tags)
        orders[id]=order
        text["stringEnums"].append(dict(id=id,variants=variants,numbers=dict(tags=tags,unknown=dict(id=invalid,argument="copy_enum"))))
    write(out/"enums.json",text)
    write(out/"enums-rust.json",dict(format="oddlang-shared-rust-v1",names=names,constantTypes={LIMITS+"VERSION":"u32"},
            variantOrder=orders,externalErrorTypes={error_id:"crate::Error"}))
    return table
