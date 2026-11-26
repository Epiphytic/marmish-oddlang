"""Derive copy encoder preflight and compare exact native error precedence."""
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
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import PIN, ROOT, read, run, sha, write
from verify import staged

PREVIOUS = runpy.run_path(str(HERE.parent/"source-copy-frame/verify.py"))
archive_at = PREVIOUS["archive_at"]
sealed = PREVIOUS["sealed"]
checked = PREVIOUS["checked"]
PORTABLE = ROOT/"oddlang/portable"
DATA = PORTABLE/"source-copy-frame-guards-v1"
RELEASED = PORTABLE/"source-copy-frame-v1"
PACKAGE = PORTABLE/"stream-source-copy-frame-guards-package.json"
RUST = PORTABLE/"copy-frame-v1/rust.json"
SOURCE = Path("crates/marmish-stream/src/copy_protocol.rs")
CALLER = HERE/"caller.rs"
METHOD = "crate::copy_protocol::Frame::encode"
CASE = "urn:marmish:copy:frame:Data"


def derive(framework, cli, checkout, target, output):
    output.mkdir()
    evidence, selection, graph = (output/name for name in ("evidence.json", "selection.json", "graph.json"))
    source = checkout/SOURCE
    package = checkout/"crates/marmish-stream"
    recipe = read(DATA/"selection.json")
    variants = ",".join(sorted(read(RUST)["cases"][case] for case in recipe["cases"]))
    with evidence.open("w") as stream:
        subprocess.run(["bash", framework/"verification/rust-recovery/run.sh", package/"Cargo.toml",
                        "marmish-stream", "lib", "marmish_stream", target, "default"],
                       stdout=stream, env=dict(os.environ, ODD_RECOVERY_TAGGED_FRAME_GUARDS=METHOD+"#"+variants), check=True)
    recipe.update(sourceSha256=sha(source), evidenceSha256=sha(evidence))
    write(selection, recipe)
    checked(cli, "recover", "frame-guards", package, source, evidence, selection,
            DATA/"template.json", RUST, graph)
    return graph


def swap_guards(original, mutant):
    archive_at(original, PIN, mutant)
    source = mutant/SOURCE
    content = source.read_text()
    count = """                if bytes.is_empty() || bytes.len() > MAX_BLOCK {
                    return Err(Error::Limit);
                }
"""
    end = "                crate::end(*n, bytes.len())?;\n"
    assert content.count(count+end) == 1
    source.write_text(content.replace(count+end, end+count))


def expected(mutant):
    edge, block = 2**63-1, 8192
    rows = []
    for offset, count in (
        (edge+1,0), (edge+1,block+1), (0,0), (0,block+1),
        (edge,1), (edge-1,1), (2**64-1,1), (edge-2,2),
        (edge,block+1), (0,3), (0,block),
    ):
        failures = dict(Limit=not 1<=count<=block, Offset=offset+count>edge)
        order = ("Offset", "Limit") if mutant else ("Limit", "Offset")
        failure = next((error for error in order if failures[error]), None)
        wire = b"\x05" + (count+8).to_bytes(4,"big") + offset.to_bytes(8,"big") + b"\x76"*count
        decode = ("Offset" if offset>edge else "Limit" if failures["Limit"] else
                  "Offset" if failures["Offset"] else "OK")
        rows.append((failure or wire.hex()) + " " + decode)
    return rows + ["Offset"]*7


def native(package, dest, lock):
    (dest/"src").mkdir(parents=True)
    shutil.copyfile(CALLER, dest/"src/main.rs")
    (dest/"Cargo.toml").write_text(
        '[package]\nname="observe-copy-guards"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nmarmish-stream={path='+json.dumps(str(package.resolve()))+'}\n[workspace]\n')
    shutil.copyfile(lock, dest/"Cargo.lock")
    run("cargo", "+1.97.1", "metadata", "--offline", "--format-version", "1", cwd=dest)
    checked("cargo", "+1.97.1", "build", "--release", "--offline", "--locked", cwd=dest)
    return run(dest/"target/release/observe-copy-guards").splitlines()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for param in ("original", "framework", "receipt"):
        parser.add_argument(param, type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    framework_pin = read(HERE/"reference-aarch64-apple-darwin.json")["framework"]
    assert run("git", "rev-parse", "HEAD", cwd=framework) == framework_pin
    assert run("git", "rev-parse", "HEAD", cwd=original) == PIN
    assert not run("git", "status", "--porcelain", cwd=original)
    rust = run("rustc", "+1.97.1", "-vV")
    host = next(line.split()[1] for line in rust.splitlines() if line.startswith("host:"))
    with tempfile.TemporaryDirectory(prefix="copy-guard-order-") as directory:
        work = Path(directory).resolve()
        fw, tools, mutant = (work/name for name in ("framework", "tools", "mutant-reference"))
        archive_at(framework, framework_pin, fw)
        checked("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
                "--target-dir", tools, cwd=fw)
        cli = tools/"debug/oddlang"
        swap_guards(original, mutant)
        recovered = derive(fw, cli, original, host, work/"baseline-recovery")
        changed = derive(fw, cli, mutant, host, work/"mutant-recovery")
        for name in ("graph.json", "evidence.json", "selection.json"):
            assert (recovered.parent/name).read_bytes() == (DATA/name).read_bytes(), name
        assert recovered.read_bytes() == (RELEASED/"graph.json").read_bytes()
        original_graph = read(recovered)
        original_graph["cases"][CASE]["encodeGuards"].reverse()
        assert read(changed) == original_graph
        proof = read(recovered.parent/"evidence.json")
        remaining = read(RELEASED/"obligations.json")
        baseline_selection = read(DATA/"selection.json")
        remaining["unresolved"] = {id: case for id,case in remaining["unresolved"].items()
                                   if id not in baseline_selection["cases"]}
        assert remaining == read(DATA/"remaining-obligations.json")

        package = staged(work, PACKAGE)
        base, altered = work/"base-package", work/"mutant-package"
        assert sealed(work, cli, package, base, original, mutant)
        assert (base/"package.lock").read_bytes() == (DATA/"package.lock").read_bytes()
        manifest = read(package)
        component, = [row for row in manifest["components"] if row["kind"]=="binary-frame"]
        (package.parent/component["graph"]["path"]).write_bytes(changed.read_bytes())
        component["graph"]["sha256"] = sha(changed)
        write(package, manifest)
        assert sealed(work, cli, package, altered, original, mutant)
        for label, crate, lock, mutated in (
            ("original", original/SOURCE.parents[1], original/"Cargo.lock", False),
            ("original-mutated", mutant/SOURCE.parents[1], mutant/"Cargo.lock", True),
            ("generated", base, base/"Cargo.lock", False),
            ("generated-mutated", altered, altered/"Cargo.lock", True),
        ):
            assert native(crate, work/label, lock) == expected(mutated), label
        write(args.receipt, dict(original=PIN, framework=framework_pin, rust=rust,
              cases=len(proof["cases"]), rules=sum(len(case["guards"]) for case in proof["cases"].values()),
              checks=4*len(expected(False)), changedOutcomes=sum(x!=y for x,y in zip(expected(False),expected(True),strict=True)),
              originalGraphSha256=sha(recovered), mutantGraphSha256=sha(changed),
              originalEvidenceSha256=sha(recovered.parent/"evidence.json"),
              mutantEvidenceSha256=sha(changed.parent/"evidence.json"),
              packageLockSha256=sha(base/"package.lock"), callerSha256=sha(CALLER),
              sourceReadDenied=True, remainingUnresolvedVariants=len(remaining["unresolved"])))
        print("Qualified nine copied preflight rules and an upstream source mutation of Data error order.")


if __name__ == "__main__":
    main()
