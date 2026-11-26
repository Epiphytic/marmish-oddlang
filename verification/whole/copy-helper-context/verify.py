"""Qualify fixed layout, bounds and errors against an unchanged native caller."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import re
import struct
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "copy-launch"))
from inputs import PIN, ROOT, consumer, observe, read, run, sha, write
from verify import project, staged

CALLER = HERE / "caller.rs"
MANIFEST = ROOT / "oddlang/portable/stream-copy-helper-context-package.json"
FRAMEWORK = "d427d5399a9ce91195eb1eb467dab99d294e45bb"
DOMAIN = b"marmish-copy-context/1\0"
ENTRYPOINT = b"__copy-helper-v1"
RETENTION = (604860, 31536000)
NAME = re.compile(rb"[A-Za-z0-9._-]{1,64}")


def error(name):
    if name == "limit":
        return {"error": ["Limit", "stream capacity reached"]}
    return {"error": [f'Invalid("{name}")', f"invalid stream input: {name}"]}


def valid(record):
    return (
        record["context_version"] == record["helper_protocol"] == 1
        and 1 <= len(record["group"]) <= 1024
        and RETENTION[0] <= record["retention_seconds"] <= RETENTION[1]
        and all(NAME.fullmatch(record[k].encode()) for k in ("expected_version", "expected_target"))
    )


def encode(record, swapped=False):
    identities = ("server", "client") if swapped else ("client", "server")
    def bits(key):
        return bytes(record[key])
    def framed(data):
        return struct.pack(">I", len(data)) + data
    return b"".join((
        DOMAIN, struct.pack(">II", record["context_version"], record["helper_protocol"]),
        *(bits(key) for key in identities), framed(bits("group")),
        *(bits(key) for key in ("session", "stream", "transfer")),
        bytes([{"put": 1, "get": 2}[record["direction"]]]), bits("launch_digest"),
        struct.pack(">Q", record["retention_seconds"]), framed(record["expected_version"].encode()),
        framed(record["expected_target"].encode()), bits("expected_commit"), framed(ENTRYPOINT),
    ))


def decode(data):
    if len(data) > 4096 or not data.startswith(DOMAIN):
        return error("copy_context")
    position = len(DOMAIN)

    def take(length):
        nonlocal position
        stop = position + length
        if stop > len(data):
            raise ValueError("copy_truncated")
        result = data[position:stop]
        position = stop
        return result

    def bounded(maximum):
        length = int.from_bytes(take(4), "big")
        if length > maximum:
            raise ValueError("limit")
        return take(length)

    def atom():
        value = bounded(64)
        if not NAME.fullmatch(value):
            raise ValueError("copy_context_build")
        return value.decode("ascii")

    try:
        record = {}
        record["context_version"], record["helper_protocol"] = struct.unpack(">II", take(8))
        for key in ("client", "server"):
            record[key] = list(take(32))
        record["group"] = list(bounded(1024))
        for key, width in (("session", 16), ("stream", 16), ("transfer", 32)):
            record[key] = list(take(width))
        tag = take(1)[0]
        if tag not in (1, 2):
            raise ValueError("copy_enum")
        record["direction"] = {1: "put", 2: "get"}[tag]
        record["launch_digest"] = list(take(32))
        record["retention_seconds"] = int.from_bytes(take(8), "big")
        record["expected_version"] = atom()
        record["expected_target"] = atom()
        record["expected_commit"] = list(take(20))
        if bounded(64) != ENTRYPOINT:
            raise ValueError("copy_context_entrypoint")
        if position != len(data):
            raise ValueError("copy_trailing")
        if not valid(record):
            raise ValueError("copy_context")
        return {"ok": record}
    except ValueError as exc:
        return error(str(exc))


def corpus():
    randomizer = random.Random(8800943)
    base = dict(context_version=1, helper_protocol=1, client=list(range(32)),
                server=list(range(32, 64)), group=[0, 255, 128], session=[42]*16,
                stream=[24]*16, transfer=[12]*32, direction="put", launch_digest=[37]*32,
                retention_seconds=RETENTION[0], expected_version="v1_.a",
                expected_target="aarch64-apple-darwin", expected_commit=[111]*20)
    constructions = [base, dict(base, direction="get")]
    for field in ("context_version", "helper_protocol"):
        constructions.extend(dict(base, **{field: value}) for value in (0, 2, 2**32-1))
    for duration in (0, RETENTION[0]-1, RETENTION[0]+1, RETENTION[1], RETENTION[1]+1, 2**64-1):
        constructions.append(dict(base, retention_seconds=duration))
    for length in (0, 1, 1023, 1024, 1025):
        constructions.append(dict(base, group=[0,255]*(length//2) + [0]*(length % 2)))
    for field in ("expected_version", "expected_target"):
        for value in ("", "a", "A-a_0.", ".", "A"*63, "B"*64, "a"*65, "v 1", "v/1", "v+1", "é", "v\0x"):
            constructions.append(dict(base, **{field: value}))
    for n in range(64):
        replacements = {key: list(randomizer.randbytes(len(base[key]))) for key in (
            "client", "server", "session", "stream", "transfer", "launch_digest", "expected_commit")}
        constructions.append(dict(base, **replacements, direction=("put","get")[n%2],
                                  group=list(randomizer.randbytes(1+n)),
                                  retention_seconds=randomizer.randint(*RETENTION)))

    sample = encode(base)
    largest = dict(base, group=[0]*1024, expected_version="a"*64, expected_target="z"*64)
    constructions.append(largest)
    maximum = encode(largest)
    wire = [encode(value) for value in constructions]
    wire.extend(sample[:size] for size in range(len(sample)))
    wire.extend(maximum[:size] for size in range(len(maximum)))
    wire.extend([sample+b"\0", sample+b"x"*(4096-len(sample)), sample+b"x"*(4097-len(sample))])
    group = len(DOMAIN)+8+64
    tag = group+4+len(base["group"])+16+16+32
    label = tag+1+32+8
    target = label+4+len(base["expected_version"])
    trailer = len(sample)-len(ENTRYPOINT)-4
    def replace(value, offset, fragment):
        return value[:offset]+fragment+value[offset+len(fragment):]

    for field in (group, label, target, trailer):
        for length in (0, 64, 65, 1024, 1025, 2**32-1):
            wire.append(replace(sample, field, struct.pack(">I", length)))
            wire.append(replace(sample[:field+4], field, struct.pack(">I", length)))
    for byte in range(256):
        wire.append(replace(sample, tag, bytes([byte])))
    for field in (label, target):
        for byte in (0, 10, 32, 43, 47, 127, 128, 255):
            wire.append(replace(sample, field+4, bytes([byte])))
    wire.append(replace(sample, trailer+4, b"?"))
    wire.append(replace(sample, 0, b"?"))
    for bad_value in (
        replace(sample, len(DOMAIN), b"\xff"),
        replace(sample, len(DOMAIN)+4, struct.pack(">I", 2)),
        replace(sample, label-8, struct.pack(">Q", 0)),
    ):
        wire.extend([replace(bad_value, tag, b"\xff"), bad_value+b"\xff",
                     replace(bad_value, label+4, b"\xff"), replace(bad_value, trailer+4, b"?")])
    wire.extend([
        replace(sample, trailer+4, b"?")+b"\x00",
        replace(sample, label+4, b"\xff")+b"\x00",
        replace(sample, tag, b"\xff")+b"\x00",
        encode(dict(base, group=[]))+b"\x00",
    ])
    for _ in range(128):
        offset = randomizer.randrange(len(sample))
        wire.append(replace(sample, offset, randomizer.randbytes(randomizer.randint(1, 6))))
    return constructions, list(dict.fromkeys(wire))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ("original", "framework", "receipt"):
        parser.add_argument(argument, type=Path)
    args = parser.parse_args()
    original, framework = args.original.resolve(), args.framework.resolve()
    for checkout, pin in ((original, PIN), (framework, FRAMEWORK)):
        assert run("git", "rev-parse", "HEAD", cwd=checkout) == pin
        assert not run("git", "status", "--porcelain", cwd=checkout)

    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp).resolve()
        run("cargo", "build", "--locked", "--offline", "--bin", "oddlang", "--target-dir",
            work/"tools", cwd=framework)
        cli = work/"tools/debug/oddlang"
        manifest = staged(work, MANIFEST)
        dest = work/"native"
        sandbox = project(work, cli, manifest, dest, original, CALLER)
        package_lock = dest/"package.lock"
        assert (MANIFEST.parent/"copy-helper-context-v1/package.lock").read_bytes() == package_lock.read_bytes()
        old = consumer(original/"crates/marmish-stream", work/"old", original/"Cargo.lock", CALLER)
        new = consumer(dest, work/"new", dest/"Cargo.lock", CALLER)

        constructions, wire = corpus()
        requests = [dict(operation="construct", record=value) for value in constructions]
        requests += [dict(operation="decode", bytes=list(value)) for value in wire]
        expected = [
            dict(validation={"ok": None}, encode={"ok": list(encode(value))})
            if valid(value) else dict(validation=error("copy_context"), encode=error("copy_context"))
            for value in constructions
        ] + [decode(value) for value in wire]
        for label, exe in (("original", old), ("generated", new)):
            for index, (observed, wanted) in enumerate(zip(observe(exe, requests), expected, strict=True)):
                assert observed == wanted, (label, index, observed, wanted)
        assert all(decode(encode(record)) == {"ok": record} for record in constructions if valid(record))

        mutated = read(manifest)
        pin = mutated["components"][-1]["graph"]
        graph_path = manifest.parent / pin["path"]
        graph = read(graph_path)
        graph["layout"][3], graph["layout"][4] = graph["layout"][4], graph["layout"][3]
        write(graph_path, graph)
        pin["sha256"] = sha(graph_path)
        write(manifest, mutated)
        mutant = work/"mutant"
        project(work, cli, manifest, mutant, original, CALLER)
        probe = consumer(mutant, work/"probe", mutant/"Cargo.lock", CALLER)
        result, = observe(probe, requests[:1])
        assert result == {"validation":{"ok":None},"encode":{"ok":list(encode(constructions[0], swapped=True))}}
        assert result != expected[0]

        write(args.receipt, dict(original=PIN, framework=FRAMEWORK, rust=run("rustc", "+1.97.1", "-vV"),
              originalAndCompanionReadDenied=sandbox, packageLockSha256=sha(package_lock), callerSha256=sha(CALLER),
              constructionCases=len(constructions), decodeCases=len(wire),
              rejectedDecodeCases=sum("error" in decode(value) for value in wire),
              observationsSha256=hashlib.sha256(json.dumps(expected, separators=(",", ":")).encode()).hexdigest(),
              negativeControl="repinned fixed-field permutation emits independently predicted swapped bytes"))
        print(f"Qualified {len(constructions)} helper-context constructions and {len(wire)} binary decodes.")


if __name__ == "__main__":
    main()
