"""Recover buffer storage, seal a generated Rust library, then stage reference tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile
import tomllib

from baseline import CASES, HERE, ORIGINAL_CASE, PIN, PREFIX, SOURCE

ROOT = HERE.parents[2]
MANIFEST = ROOT/"oddlang/portable/prototype-buffer-package.json"
GRAPH = MANIFEST.parent/"prototype-buffer-v1/graph.json"
OVERLAY = GRAPH.with_name("rust.json")
from upstream_case import TEST_HASH, original_test as extract_original_test
NATIVE_TEST_PREFIX = "buffer::__projection_0::"
def original_test(original):
    return extract_original_test(original, PIN, SOURCE)


CORE = runpy.run_path(str(HERE.parent/"core-package/verify.py"))
WORKSPACE = runpy.run_path(str(HERE.parent/"workspace/verify.py"))


def run(*cmd, **kw):
    return subprocess.check_output(cmd, **kw)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def erasure_provider(original, host):
    metadata = json.loads(run("cargo","+1.97.1","metadata","--locked","--offline","--format-version","1",
                              "--filter-platform",host,"--manifest-path",original/"Cargo.toml"))
    crate, = [p for p in metadata["packages"] if Path(p["manifest_path"]).resolve() ==
              (original/"crates/marmish-prototype/Cargo.toml").resolve()]
    node, = [n for n in metadata["resolve"]["nodes"] if n["id"]==crate["id"]]
    edge, = [d for d in node["deps"] if d["name"]=="zeroize"]
    provider, = [p for p in metadata["packages"] if p["id"]==edge["pkg"]]
    def identity(lock):
        item, = [p for p in lock["package"] if p["name"]==provider["name"] and p["version"]==provider["version"]]
        return {k:item[k] for k in ("name","version","source","checksum")}
    expected = identity(tomllib.loads((GRAPH.parent/"Cargo.lock").read_text()))
    selected = identity(tomllib.loads((original/"Cargo.lock").read_text()))
    if selected!=expected or provider["source"]!=selected["source"] or selected["name"]!="zeroize":
        raise ValueError("original storage provider differs from the released native package")
    return selected


def recover(work, framework, cli, checkout, host):
    raw = run("bash",framework/"verification/rust-recovery/run.sh",
              checkout/"crates/marmish-prototype/Cargo.toml","marmish-prototype","lib",
              "marmish_prototype",host,"default",env=dict(os.environ,ODD_RECOVERY_DATA_SHAPES="1"))
    obs = work/"observation.json"
    obs.write_bytes(raw)
    recovered = work/"recovered"
    run(cli,"recover","buffered-input",obs,HERE/"selection.json",recovered)
    return recovered


def project(work, inputs, cli, original, framework, host, denied):
    """Allow only the copied CLI, portable inputs and the offline native toolchain."""
    compiler = work/"oddlang"
    shutil.copy2(cli,compiler)
    if platform.system() == "Linux":
        prefix, env = WORKSPACE["linux_isolation"](work)
    else:
        roots, env = CORE["isolation_environment"](work,host)
        prefix = CORE["isolated"](work,roots,ROOT,original,framework)
    if run(*prefix,"/bin/cat",inputs/MANIFEST.name,cwd=work,env=env) != (inputs/MANIFEST.name).read_bytes():
        raise ValueError("staged inputs could not be read inside the build")
    for hidden in [ROOT/"verification/whole/private-buffer/observations.rs", original/SOURCE, framework/"src/lib.rs", *denied]:
        check = subprocess.run([*prefix,"/bin/cat",hidden],env=env,capture_output=True)
        if not hidden.is_file() or check.returncode == 0 or not any(
                error in check.stderr for error in [b"No such file",b"Operation not permitted"]):
            raise ValueError(f"isolation failed for {hidden}")
    product = work/"product"
    run(*prefix,compiler,"build","native-package","rust",inputs/MANIFEST.name,product,cwd=work,env=env)
    lock = CORE["verify_lock"](product,inputs/MANIFEST.name).read_bytes()
    run(*prefix,"cargo","build","--release","--offline","--locked","--manifest-path",product/"Cargo.toml",
        env=env,cwd=work)
    return product,sha(lock)


def execute(product, name, *, failure=None):
    exact = NATIVE_TEST_PREFIX+name
    result = subprocess.run(["cargo","+1.97.1","test","--release","--offline","--locked","--lib","--",
                             "--exact",exact],cwd=product,capture_output=True,text=True)
    wanted = "ok" if failure is None else "FAILED"
    if (result.returncode != (0 if failure is None else 101)
            or result.stdout.splitlines().count(f"test {exact} ... {wanted}") != 1
            or (failure and failure not in result.stdout)):
        raise RuntimeError(result.stdout+result.stderr)
    return result.returncode


def stage_and_test(work,cli,original,framework,host,graph,mode):
    inputs = work/"inputs"
    inputs.mkdir()
    manifest = json.loads(MANIFEST.read_bytes())
    component, = manifest["components"]
    for key in (manifest["cargoLock"],component["graph"],component["overlay"]):
        dest = inputs/key["path"]
        dest.parent.mkdir(exist_ok=True,parents=True)
        data = (MANIFEST.parent/key["path"]).read_bytes()
        if sha(data) != key["sha256"]:
            raise ValueError("input pin mismatch")
        if key is component["graph"]:
            data = graph
            key["sha256"] = sha(data)
        dest.write_bytes(data)
    local_manifest = inputs/MANIFEST.name
    local_manifest.write_bytes(MANIFEST.read_bytes() if mode=="reference" else json.dumps(manifest).encode())
    denied = [work.parent/"reference/observation.json"]
    alternate = work.parent/"mutant-checkout"/SOURCE
    if alternate.exists():
        denied.append(alternate)
    # A direct /tmp child avoids root-owned, inaccessible synthetic mount parents in bwrap.
    scratch_parent = "/tmp" if platform.system()=="Linux" else None
    with tempfile.TemporaryDirectory(dir=scratch_parent) as directory:
        sealed = Path(directory).resolve()
        staged = sealed/"inputs"
        shutil.move(str(inputs),staged)
        built, lock = project(sealed,staged,cli,original,framework,host,denied)
        product = work/"product"
        shutil.move(str(built),product)
    leaf = product/"src/group_0/__projection_0.rs"
    with leaf.open("ab") as output:
        output.write(original_test(original))
        if mode != "extent":
            output.write((HERE/"observations.rs").read_bytes())
    if mode == "extent":
        result = subprocess.run(["cargo","+1.97.1","test","--release","--locked","--offline","--no-run",
                                 "--lib","--message-format=json"],cwd=product,capture_output=True,text=True)
        errors = [row["message"] for line in result.stdout.splitlines()
                  if (row:=json.loads(line)).get("reason")=="compiler-message" and row["message"]["level"]=="error"]
        if (result.returncode!=101 or len(errors)!=1 or (errors[0]["code"] or {}).get("code")!="E0308"
                or not any("Zeroizing::new([0; 8192])" in t["text"] for s in errors[0]["spans"]
                           if s["is_primary"] for t in s["text"])
                or "4096" not in errors[0]["rendered"]):
            raise ValueError(result.stdout+result.stderr)
        return {"packageLockSha256":lock,"originalTestDiagnostic":"E0308","expectedExtent":4096}
    if mode=="failure-policy":
        return {"packageLockSha256":lock, "exit":execute(product,PREFIX+CASES[1],failure="failed refill preserves old end")}
    if mode=="wipe-policy":
        return {"packageLockSha256":lock,
                "holdoutExit":execute(product,PREFIX+CASES[0],failure="consumed span must be erased"),
                "originalExit":execute(product,ORIGINAL_CASE,failure="input.bytes[..8].iter().all")}
    if mode=="empty-policy":
        return {"packageLockSha256":lock, "exit":execute(product,PREFIX+CASES[2],failure="empty destination must still poll exhausted input")}
    names = [ORIGINAL_CASE]+[PREFIX+n for n in CASES]
    for name in names:
        execute(product,name)
    return {"packageLockSha256":lock, "passed":names, "generatedPrefix":NATIVE_TEST_PREFIX}


def verify(original, framework, cli, host):
    witnessed = json.loads((HERE/f"original-{host}.json").read_bytes())
    names = [ORIGINAL_CASE]+[PREFIX+n for n in CASES]
    if (witnessed["revision"]!=PIN or witnessed["target"]!=host or witnessed["passed"]!=names or
            witnessed["upstreamFileSha256"]!=sha(run("git","-C",original,"show",f"{PIN}:{SOURCE}")) or
            witnessed["callerSha256"]!=sha((HERE/"observations.rs").read_bytes())):
        raise ValueError("original observation receipt mismatch")
    provider = erasure_provider(original,host)
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory).resolve()
        for name in ["reference","changed-source","changed-policy","wipe-policy","empty-policy"]:
            (work/name).mkdir()
        baseline = recover(work/"reference",framework,cli,original,host)
        graph = (baseline/"graph.json").read_bytes()
        if graph != GRAPH.read_bytes() or (baseline/"rust.json").read_bytes()!=OVERLAY.read_bytes():
            raise ValueError("recovered baseline drift")
        normal = stage_and_test(work/"reference",cli,original,framework,host,graph,"reference")
        checkout = work/"mutant-checkout"
        run("git","clone","-q","--local","--no-checkout",original,checkout)
        run("git","-C",checkout,"checkout","-q","--detach",PIN)
        source = checkout/SOURCE
        old = source.read_bytes()
        # Change the actual fixed storage declaration and its initializers together.
        if old.count(b"[u8; 8192]")!=1 or old.count(b"[0; 8192]")!=2:
            raise ValueError("unsupported original extent occurrence count")
        source.write_bytes(old.replace(b"[u8; 8192]",b"[u8; 4096]").replace(b"[0; 8192]",b"[0; 4096]"))
        changed = recover(work/"changed-source",framework,cli,checkout,host)
        smaller = (changed/"graph.json").read_bytes()
        expect = json.loads(graph)
        expect["capacity"] = 4096
        if json.loads(smaller)!=expect or (changed/"rust.json").read_bytes()!=OVERLAY.read_bytes():
            raise ValueError("changed source did not recover only its new extent")
        extent = stage_and_test(work/"changed-source",cli,original,framework,host,smaller,"extent")
        policy = json.loads(graph)
        policy["transitions"]["refillError"] = "discard-window"
        failure = stage_and_test(work/"changed-policy",cli,original,framework,host,json.dumps(policy).encode(),"failure-policy")
        controls = {}
        for mode,setting,value in [("wipe-policy","consumedBytes","retain"),("empty-policy","emptyRead","return-zero")]:
            changed_policy = json.loads(graph)
            changed_policy["transitions"][setting] = value
            controls[setting] = stage_and_test(work/mode,cli,original,framework,host,json.dumps(changed_policy).encode(),mode)
        return dict(target=host,original=PIN,additionalPolicyControls=controls,erasureProvider=provider,graphSha256=sha(graph),overlaySha256=sha(OVERLAY.read_bytes()),
                    originalCaseSha256=TEST_HASH,observationsSha256=sha((HERE/"observations.rs").read_bytes()),
                    baseline=normal,sourceWidthMutation=extent,refillPolicyMutation=failure,
                    recovery=json.loads((baseline/"recovery.lock").read_bytes()))


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("original","framework","compiler_cache","receipt"):
        ap.add_argument(name,type=Path)
    args = ap.parse_args()
    original,framework = args.original.resolve(),args.framework.resolve()
    code = json.loads((HERE/"compiler.json").read_bytes())["revision"]
    for path,rev in [(original,PIN),(framework,code)]:
        if run("git","-C",path,"rev-parse","HEAD").decode().strip()!=rev or run("git","-C",path,"status","--porcelain"):
            raise ValueError(f"expected clean pinned checkout at {path}")
    run("cargo","+1.96.1","build","--offline","--locked","--bin","oddlang","--target-dir",args.compiler_cache,cwd=framework,env=CORE["cargo_environment"]())
    target = next(line[6:] for line in run("rustc","+1.97.1","-vV").decode().splitlines() if line.startswith("host: "))
    result = verify(original,framework,args.compiler_cache.resolve()/"debug/oddlang",target)
    result.update(framework=code, frameworkCompiler=run("rustc","+1.96.1","--version").decode().strip(),
                  productCompiler=run("rustc","+1.97.1","--version").decode().strip())
    args.receipt.write_text(json.dumps(result,indent=2)+"\n")
