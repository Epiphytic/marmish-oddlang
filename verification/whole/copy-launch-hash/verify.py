"""Compare an independently encoded corpus with original and generated Launch views."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import struct
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import HERE as DATA_VERIFIER
from inputs import PIN, ROOT, consumer, observe, read, run, sha, write
from verify import canonical, corpus as data_corpus, project, staged

MANIFEST = ROOT / "oddlang/portable/stream-launch-hash-package.json"
FRAMEWORK = "217da3c13d504b622e0be4efce2eaf8bb51b05f0"
CALLER = HERE / "caller.rs"


def corpus():
    rand = random.Random(535492)
    base = dict(version=1, mode="create", transfer=list(range(32)),
                client=list(range(64, 96)), server=list(range(128, 160)),
                direction="put", source=[255, 128, 65], destination=[1, 2],
                overwrite=False, preserve_mode=False, preserve_mtime=False, codec="none")
    flags = ("overwrite", "preserve_mode", "preserve_mtime")
    cases = [dict(base, mode=m, direction=d, codec=c, **dict(zip(flags, f)))
             for m, d, c, f in itertools.product(("create", "resume"), ("put", "get"),
                                                  ("none", "zlib"), itertools.product((False, True), repeat=3))]
    for role in ("transfer", "client", "server"):
        cases.append(dict(base, **{role: list(reversed(base[role]))}))
    for role in ("source", "destination"):
        for path in ([], [0], [1, 0], [255], [254]*4095, [255]*4096, [255]*4097):
            cases.extend(dict(base, version=v, **{role: path}) for v in (0, 1))
    cases.extend(dict(base, version=v, source=[], destination=[0]) for v in (0, 1, 2, 2**32-1))
    for _ in range(128):
        binary = {role: list(rand.randbytes(32)) for role in ("transfer", "client", "server")}
        paths = {role: [rand.randint(1, 255) for _ in range(rand.randint(1, 96))]
                 for role in ("source", "destination")}
        cases.append(dict(rand.choice(cases[:64]), **binary, **paths))
    return cases


def expected(req, swapped=False):
    def bytes_at(key): return bytes(req[key])
    def length_prefixed(key):
        path = bytes_at(key)
        return struct.pack(">I", len(path)) + path
    def sha256(data): return hashlib.sha256(data)
    transfer, client, server = (bytes_at(key) for key in ("transfer", "client", "server"))
    direction = {"put": b"\x01", "get": b"\x02"}[req["direction"]]
    principals = (server, client) if swapped else (client, server)
    namespace = ".marmish-copy-v1-" + sha256(b"".join((
        b"marmish-copy-state/1\0", *principals, transfer, direction
    ))).hexdigest()
    if req["version"] != 1:
        error = "copy_version"
    elif any(not 1 <= len(req[key]) <= 4096 or 0 in req[key] for key in ("source", "destination")):
        error = "copy_path"
    else:
        error = None
    if error:
        digest = {"error": [f'Invalid("{error}")', f"invalid stream input: {error}"]}
    else:
        mode = {"create": b"\x01", "resume": b"\x02"}[req["mode"]]
        codec = {"none": b"\x00", "zlib": b"\x01"}[req["codec"]]
        flags = bytes(req[k] for k in ("overwrite", "preserve_mode", "preserve_mtime"))
        identities = (client, transfer, server) if swapped else (transfer, client, server)
        preimage = b"".join((b"marmish-copy-launch/1\0", struct.pack(">I", req["version"]), mode,
                            *identities, direction, length_prefixed("source"),
                            length_prefixed("destination"), flags, codec))
        digest = {"ok": list(sha256(preimage).digest())}
    return {"digest": digest, "namespace": namespace}


def check_base():
    base = MANIFEST.parent / "copy-launch-v1"
    graphs = MANIFEST.parent / "copy-launch-hash-v1"
    contract = read(graphs / "launch.json")
    tags = contract.pop("hashTags")
    contract.pop("hashViews")
    codec = "urn:marmish:stream:codec"
    contract["types"].remove({"kind": "external", "id": codec})
    assert contract == read(base / "launch.json")
    for enum in read(base / "enums.json")["stringEnums"]:
        if enum["id"] in tags:
            assert tags[enum["id"]] == enum["numbers"]["tags"]
    hints = read(graphs / "launch-rust.json")
    del hints["hashViews"], hints["hashVariants"], hints["externals"][codec]
    assert hints == read(base / "launch-rust.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("original", "framework", "receipt"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    assert run("git", "rev-parse", "HEAD", cwd=original) == PIN
    assert not run("git", "status", "--porcelain", cwd=original)
    assert run("git", "rev-parse", "HEAD", cwd=framework) == FRAMEWORK
    assert not run("git", "status", "--porcelain", cwd=framework)
    check_base()

    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        run("cargo", "build", "--locked", "--offline", "--bin", "oddlang", "--target-dir", work/"tools",
            cwd=framework)
        cli = work / "tools/debug/oddlang"
        manifest = staged(work, MANIFEST)
        dest = work / "package"
        sandbox = project(work, cli, manifest, dest, original, CALLER)
        package_lock = dest / "package.lock"
        assert package_lock.read_bytes() == (MANIFEST.parent / "copy-launch-hash-v1/package.lock").read_bytes()

        old = consumer(original/"crates/marmish-stream", work/"original", original/"Cargo.lock", CALLER)
        new = consumer(dest, work/"generated", dest/"Cargo.lock", CALLER)
        requests = corpus()
        oracle = [expected(req) for req in requests]
        assert observe(old, requests) == oracle == observe(new, requests)
        assert len({row["namespace"] for row in oracle[:64]}) == 2
        assert len({bytes(row["digest"]["ok"]) for row in oracle[:64]}) == 64

        data_caller = DATA_VERIFIER / "caller.rs"
        old_data = consumer(original/"crates/marmish-stream", work/"original-data",
                            original/"Cargo.lock", data_caller)
        new_data = consumer(dest, work/"generated-data", dest/"Cargo.lock", data_caller)
        creates, decodes = data_corpus()
        assert observe(old_data, creates) == [canonical(req) for req in creates] == observe(new_data, creates)
        assert observe(old_data, decodes) == observe(new_data, decodes)

        mutated = read(manifest)
        pin = mutated["components"][-1]["graph"]
        graph_path = manifest.parent / pin["path"]
        graph = read(graph_path)
        for view in graph["hashViews"]:
            first, second = (2, 3) if view["output"]["kind"] == "bytes" else (0, 1)
            view["fields"][first], view["fields"][second] = view["fields"][second], view["fields"][first]
        write(graph_path, graph)
        pin["sha256"] = sha(graph_path)
        write(manifest, mutated)
        mutant = work / "mutated"
        project(work, cli, manifest, mutant, original, CALLER)
        wrong = consumer(mutant, work/"mutant-observer", mutant/"Cargo.lock", CALLER)
        got, = observe(wrong, requests[:1])
        assert got == expected(requests[0], swapped=True)
        assert all(got[key] != oracle[0][key] for key in ("digest", "namespace"))
        write(args.receipt, dict(original=PIN, framework=FRAMEWORK, rust=run("rustc", "+1.97.1", "-vV"),
             originalAndCompanionReadDenied=sandbox, packageLockSha256=sha(package_lock),
             callerSha256=sha(CALLER), cases=len(requests), invalidDigestCases=sum("error" in r["digest"] for r in oracle),
             constructCases=len(creates), exactDecodeCases=len(decodes),
             expectationsSha256=hashlib.sha256(json.dumps(oracle, separators=(",", ":")).encode()).hexdigest(),
             negativeControl="reordering fixed-width field slots changes digest and namespace"))
        print(f"Qualified both Launch views across {len(requests)} original, projected and independent observations.")


if __name__ == "__main__":
    main()
