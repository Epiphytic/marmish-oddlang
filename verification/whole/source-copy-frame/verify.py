"""Recover frame encoder structure and test original and generated native bytes."""
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
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import archive_at, PIN, ROOT, read, run, sha, write
from verify import linux_project, staged

PORTABLE = ROOT / "oddlang/portable"
DATA = PORTABLE / "source-copy-frame-v1"
MANIFEST = PORTABLE / "stream-source-copy-frame-package.json"
RELEASED = PORTABLE / "copy-frame-v1"
CALLER = HERE / "caller.rs"
SOURCE = Path("crates/marmish-stream/src/copy_protocol.rs")
METHOD = "crate::copy_protocol::Frame::encode"
CASE = "urn:marmish:copy:frame:Finish"


def checked(*command, cwd=None, env=None):
    return subprocess.run(command, cwd=cwd, env=env, check=True)


def observe(framework, cli, checkout, target, output):
    output.mkdir()
    evidence, selection = (output / name for name in ("evidence.json", "selection.json"))
    source = checkout / SOURCE
    with evidence.open("w") as sink:
        subprocess.run(
                ["bash", framework / "verification/rust-recovery/run.sh",
                 checkout / "crates/marmish-stream/Cargo.toml", "marmish-stream", "lib", "marmish_stream",
                 target, "default"], env=dict(os.environ, ODD_RECOVERY_TAGGED_FRAME=METHOD), stdout=sink, check=True)
    inputs = read(DATA / "selection.json")
    inputs.update(sourceSha256=sha(source), evidenceSha256=sha(evidence))
    write(selection, inputs)
    recovered = output / "recovered"
    checked(cli, "recover", "tagged-frame", checkout / "crates/marmish-stream", source,
            evidence, selection, DATA / "template.json", RELEASED / "rust.json", recovered)
    for path in recovered.iterdir():
        path.rename(output / path.name)
    return output / "graph.json"


def sealed(work, cli, manifest, output, original, mutant):
    assert not list(manifest.parent.rglob("*.rs"))
    assert not list(manifest.parent.rglob("evidence.json"))
    if platform.system() == "Linux":
        return linux_project(cli, manifest, output, original, CALLER, extra_sources=(mutant,))
    assert platform.system() == "Darwin"
    recovered = (work / "baseline-recovery", work / "mutant-recovery")
    private = (ROOT, original, mutant, work / "framework", *recovered)
    profile = "(version 1)(allow default)(deny network*)" + "".join(
        "(deny file-read* (subpath " + json.dumps(str(path)) + "))" for path in private)
    sandbox = ["/usr/bin/sandbox-exec", "-p", profile]
    for path in (original / SOURCE, mutant / SOURCE, CALLER, DATA / "evidence.json",
                 *(item / "evidence.json" for item in recovered), work / "framework/Cargo.toml"):
        result = subprocess.run([*sandbox, "/bin/cat", path], capture_output=True)
        assert result.returncode and b"Operation not permitted" in result.stderr
    checked(*sandbox, cli, "build", "native-package", "rust", manifest, output, cwd=work)
    checked(*sandbox, "cargo", "+1.97.1", "build", "--locked", "--offline", "--release", cwd=output)
    return True


def alter(original, mutant):
    archive_at(original, PIN, mutant)
    source = mutant / SOURCE
    text = source.read_text()
    before = """            Self::Finish { transfer, sha256 } => {
                payload.extend(transfer);
                payload.extend(sha256);
                9
            }"""
    after = """            Self::Finish { transfer, sha256 } => {
                payload.extend(sha256);
                payload.extend(transfer);
                19
            }"""
    assert text.count(before) == 1
    source.write_text(text.replace(before, after))


def oracle(mutated):
    def uint(n, length=8):
        return n.to_bytes(length, "big")
    def envelope(tag, payload):
        return (bytes([tag]) + uint(len(payload), 4) + payload).hex()
    transfer, digest = bytes([0x22])*32, bytes([0x45])*32
    owners = transfer + bytes([0x38])*32 + bytes([0x74])*32
    paths = uint(2, 4) + b"\xf1\x81" + uint(1, 4) + b"\xde"
    metadata = b"\x01" + paths + b"\x01\x00\x00"
    rows = [
        (1, uint(1, 4) + b"\x01" + owners + metadata + b"\x01"),
        (2, uint(1, 4) + owners + metadata + uint(0x0654) + digest + b"\x00\x00"),
        (3, b"\x02" + uint(0x1234) + digest),
        (4, uint(0x1334) + digest),
        (5, uint(0x16) + b"\x81\x26\x59"),
        (6, uint(0x1534) + digest),
        (7, uint(0x1634) + digest),
        (8, transfer + uint(0x1734) + digest + b"\x01"),
        (19, digest + transfer) if mutated else (9, transfer + digest),
        (10, b"\x02" + uint(0x07) + uint(0x0b)),
        (11, b"\x09\x00"),
    ]
    return [envelope(tag, payload) for tag, payload in rows]


def native(crate, work, lock):
    (work / "src").mkdir(parents=True)
    shutil.copyfile(CALLER, work / "src/main.rs")
    (work / "Cargo.toml").write_text(
        '[package]\nname="observe-copy-frames"\nversion="0.1.0"\nedition="2021"\n'
        '[dependencies]\nmarmish-stream={path=' + json.dumps(str(crate.resolve())) + '}\n[workspace]\n')
    shutil.copyfile(lock, work / "Cargo.lock")
    run("cargo", "+1.97.1", "metadata", "--offline", "--format-version", "1", cwd=work)
    checked("cargo", "+1.97.1", "build", "--release", "--offline", "--locked", cwd=work)
    return run(work / "target/release/observe-copy-frames").splitlines()


def projection(lock):
    item, = [component for component in read(lock)["components"] if component["kind"] == "binary-frame"]
    return item["sourceSha256"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for label in ("original", "framework", "receipt"):
        parser.add_argument(label, type=Path)
    args = parser.parse_args()
    original = args.original.resolve()
    framework = args.framework.resolve()
    pin = read(HERE / "reference-aarch64-apple-darwin.json")["framework"]
    assert run("git", "rev-parse", "HEAD", cwd=framework) == pin
    assert run("git", "rev-parse", "HEAD", cwd=original) == PIN
    assert not run("git", "status", "--porcelain", cwd=original)
    rust = run("rustc", "+1.97.1", "-vV")
    host = next(line.split()[1] for line in rust.splitlines() if line.startswith("host:"))
    with tempfile.TemporaryDirectory(prefix="source-copy-frame-") as directory:
        work = Path(directory).resolve()
        fw, tools, mutant = (work/name for name in ("framework", "tools", "mutant-reference"))
        archive_at(framework, pin, fw)
        checked("cargo", "+1.97.1", "build", "--locked", "--offline", "--bin", "oddlang",
                "--target-dir", tools, cwd=fw)
        cli = tools / "debug/oddlang"
        alter(original, mutant)
        original_graph = observe(fw, cli, original, host, work/"baseline-recovery")
        mutated_graph = observe(fw, cli, mutant, host, work/"mutant-recovery")
        for artifact in (original_graph, original_graph.parent/"evidence.json", original_graph.parent/"selection.json", original_graph.parent/"obligations.json"):
            assert artifact.read_bytes() == (DATA / artifact.name).read_bytes(), artifact.name
        old = read(RELEASED/"graph.json")
        for case in old["cases"].values():
            case.setdefault("encodeGuards", [])
            case.setdefault("decodeGuards", [])
        assert read(original_graph) == old
        old["cases"][CASE]["tag"] = 19
        old["cases"][CASE]["slots"].reverse()
        assert read(mutated_graph) == old
        manifest = staged(work, MANIFEST)
        base, changed = (work / name for name in ("base-package", "changed-package"))
        assert sealed(work, cli, manifest, base, original, mutant)
        assert (base/"package.lock").read_bytes() == (DATA/"package.lock").read_bytes()
        staged_manifest = read(manifest)
        part, = [part for part in staged_manifest["components"] if part["kind"] == "binary-frame"]
        (manifest.parent / part["graph"]["path"]).write_bytes(mutated_graph.read_bytes())
        part["graph"]["sha256"] = sha(mutated_graph)
        write(manifest, staged_manifest)
        assert sealed(work, cli, manifest, changed, original, mutant)
        for label, crate, cargo_lock, mutated in (
            ("original", original / SOURCE.parents[1], original / "Cargo.lock", False),
            ("original-mutated", mutant / SOURCE.parents[1], mutant / "Cargo.lock", True),
            ("generated", base, base / "Cargo.lock", False),
            ("generated-mutated", changed, changed / "Cargo.lock", True),
        ):
            assert native(crate, work / label, cargo_lock) == oracle(mutated), label
        base_emitted, mutated_emitted = projection(base/"package.lock"), projection(changed/"package.lock")
        assert base_emitted != mutated_emitted
        receipt = read(original_graph.parent/"evidence.json")
        write(args.receipt, dict(original=PIN, framework=pin, rust=rust,
              cases=len(receipt["cases"]), writes=sum(len(row["writes"]) for row in receipt["cases"]),
              unresolved={row["variant"]:row["unresolved"] for row in receipt["cases"] if row["unresolved"]},
              checks=4*len(oracle(False)), mutatedFrames=sum(a!=b for a,b in zip(oracle(False),oracle(True),strict=True)),
              originalGraphSha256=sha(original_graph), mutatedGraphSha256=sha(mutated_graph),
              originalEvidenceSha256=sha(original_graph.parent/"evidence.json"),
              mutatedEvidenceSha256=sha(mutated_graph.parent/"evidence.json"),
              originalNativeArtifactSha256=base_emitted, mutatedNativeArtifactSha256=mutated_emitted,
              packageLockSha256=sha(base/"package.lock"), callerSha256=sha(CALLER), sourceReadDenied=True))
        print("Qualified all eleven upstream copy frame variants and a source-mutated native payload.")


if __name__ == "__main__":
    main()
