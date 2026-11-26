"""Exercise unaltered native release parsing and one pinned source mutation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from corpus import cases, mutant_expected

HERE = Path(__file__).resolve().parent
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
NATIVE = Path("crates/marmish-native/src")
SOURCE = NATIVE / "native_manifest.rs"
UNCHANGED = ("native_manifest::tests::strict_vectors",
             "native_manifest::tests::shared_release_version_vectors")
PROBE = "manifest_oracle::manifest_contract_probe"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(*args, cwd=None, env=None):
    p = subprocess.run(args,cwd=cwd,env=env,capture_output=True,timeout=1200)
    if p.returncode:
        raise RuntimeError(p.stdout.decode(errors="replace")+p.stderr.decode(errors="replace"))
    return p.stdout


def select(source, target, test, env):
    output = run("cargo","+1.97.1","test","--release","--locked","--offline","--target-dir",target,
                 "-p","marmish-native","--lib",test,"--","--exact",cwd=source,env=env)
    witnessed = f"test {test} ... ok"
    if output.decode().splitlines().count(witnessed) != 1 or b"1 passed; 0 failed" not in output:
        raise ValueError(f"missing exact native test {test}")
    return witnessed


def qualify(original, target, base):
    if run("git","rev-parse","HEAD",cwd=original).decode().strip() != ORIGINAL:
        raise ValueError("unexpected pinned upstream revision")
    scratch=base/"original"
    scratch.mkdir()
    archive=base/"source.tar"
    with archive.open("wb") as stream:
        subprocess.run(["git","-C",original,"archive",ORIGINAL],stdout=stream,check=True)
    run("tar","-xf",archive,"-C",scratch)
    pinned = (scratch/SOURCE).read_bytes()
    lib = scratch/NATIVE/"lib.rs"
    with lib.open("ab") as stream:
        stream.write(b'\n#[cfg(test)]\n#[path = "manifest_oracle.rs"]\nmod manifest_oracle;\n')
    (scratch/NATIVE/"manifest_oracle.rs").write_bytes((HERE/"probe.rs").read_bytes())
    fixtures=cases()
    inputs=base/"cases.json"
    inputs.write_text(json.dumps([{"case":name,"hex":raw.hex()} for name,(raw,_) in fixtures.items()]))
    env = dict(os.environ,CARGO_NET_OFFLINE="true",MARMISH_BUILD_COMMIT=ORIGINAL)
    for variable in ("CARGO_BUILD_TARGET","CARGO_TARGET_DIR","RUSTFLAGS","CARGO_ENCODED_RUSTFLAGS"):
        env.pop(variable,None)
    witnesses=[select(scratch,target,name,env) for name in UNCHANGED]

    def observe(name, expected):
        result=base/f"{name}.json"
        select(scratch,target,PROBE,dict(env,MANIFEST_CASES=str(inputs),MANIFEST_RESULT=str(result)))
        rows=json.loads(result.read_bytes())
        if [row["case"] for row in rows] != list(expected):
            raise ValueError("missing, extra or reordered case")
        for row in rows:
            if row["result"] != expected[row["case"]]:
                raise ValueError(f"{name} {row['case']}: {row['result']!r} != {expected[row['case']]!r}")
        return rows

    baseline = observe("baseline",{name:value for name,(_,value) in fixtures.items()})
    old,new=b"                    16 * 1024 * 1024\n",b"                    16 * 1024 * 1024 + 1\n"
    if pinned.count(old) != 1:
        raise ValueError("mutation requires exactly one license ceiling")
    mutation=pinned.replace(old,new)
    (scratch/SOURCE).write_bytes(mutation)
    mutated=observe("mutated",mutant_expected(fixtures))
    changed=[r["case"] for r,s in zip(baseline,mutated) if r != s]
    if changed!=["size-3-16777217"]:
        raise ValueError("mutation did not produce the independent expected delta")
    return {
        "format":"marmish-release-manifest-original-observation-v1",
        "originalRevision":ORIGINAL,
        "compiler":run("rustc","+1.97.1","-vV").decode().splitlines(),
        "sourcePins":{str(p):sha(run("git","show",f"{ORIGINAL}:{p}",cwd=original)) for p in
                       (SOURCE,NATIVE/"foundation.rs",NATIVE/"error.rs",Path("tools/release-version-vectors.json"))},
        "mutatedSourceSha256":sha(mutation),
        "callerSha256":sha((HERE/"probe.rs").read_bytes()),
        "corpusSha256":sha((HERE/"corpus.py").read_bytes()),
        "inputsSha256":sha(inputs.read_bytes()),
        "unchangedPureTests":witnesses,
        "cases":len(baseline),
        "accepted":sum("error" not in r["result"] for r in baseline),
        "changedCases":changed,
        "baseline":baseline,
        "mutation":[r for r in mutated if r["case"] in changed],
    }


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("original","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("--target-dir",type=Path,required=True)
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="odd-manifest-oracle-") as d:
        evidence=qualify(args.original.resolve(),args.target_dir.resolve(),Path(d).resolve())
    args.output.write_text(json.dumps(evidence,indent=2)+"\n")


if __name__=="__main__":
    main()
