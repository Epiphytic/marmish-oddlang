"""Compare the request codec across isolated reference and graph-built consumers."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
MODEL = ROOT.parents[2] / "oddlang/portable/command-requests-v1"
PIN = "af54339d250290490d62d4475a77539b7e739071"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, **kwargs).stdout


def corpus():
    account = "11" * 32
    common = dict(client=account, id="66" * 16, server="22" * 32, session="44" * 16,
                  protocol="marmish-command/1", type="request")
    invocation = dict(invocation="55" * 16)
    bodies = {
        "submit": dict(invocation, argv=[base64.b64encode(b"/usr/bin/true").decode(), "/w=="], stdin="AAo="),
        "status": invocation, "cancel": invocation,
        "poll": dict(invocation, stdout_cursor=9007199254740993, stderr_cursor=2**63-1, max_bytes=16384),
        "finish": dict(invocation, stdout_cursor=1, stderr_cursor=0, submit_sha256="42"*32, terminal_sha256="05"*32),
    }
    def compact(value):
        return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()

    for op, body in bodies.items():
        frame = dict(common, op=op, body=body)
        raw = compact(frame)
        yield op, "canonical", raw, True
        yield op, "trailing-lf", raw+b"\n", False
        yield op, "duplicate-op", raw.replace(b'"op":', b'"op":"status","op":', 1), False
        yield op, "same-account", compact(dict(frame, server=account)), False
        yield op, "unknown-body-field", compact(dict(frame, body=dict(body, extra=0))), False
        yield op, "oversize", raw.ljust(32769), False
        for field, value in [("protocol", "other"), ("type", "response"), ("id", "AA"*16),
                             ("session", "44"*15)]:
            yield op, "wrong-"+field, compact(dict(frame, **{field:value})), False
        missing = dict(frame)
        del missing["server"]
        yield op, "missing-peer", compact(missing), False
        if op == "finish":
            yield op, "cursor-overflow", compact(dict(frame, body=dict(body, stderr_cursor=2**63))), False
        if op == "poll":
            for k, n in [("max_bytes", 0), ("max_bytes", 16385), ("stdout_cursor", 2**63)]:
                yield op, f"{k}-{n}", compact(dict(frame, body=dict(body, **{k:n}))), False
        if op == "submit":
            for size in (32768, 32769):
                for empty_args in range(4):
                    payload = dict(body, argv=[""]*(1+empty_args), stdin="YWFh"*4000)
                    remaining = size-len(compact(dict(frame, body=payload)))
                    if remaining % 4 == 0:
                        payload["argv"][0] = "YWFh"*(remaining//4)
                        exact = compact(dict(frame, body=payload))
                        assert len(exact) == size
                        yield op, f"wire-bytes-{size}", exact, size == 32768
                        break
                else:
                    raise AssertionError("unable to construct wire boundary")
            for name, value in [("empty-list", []), ("empty-executable", [""]), ("nul", ["AA=="]),
                                ("noncanonical-base64", ["Zh=="]), ("too-many", ["YQ=="]*129)]:
                yield op, name, compact(dict(frame, body=dict(body, argv=value))), False


def observe(root, name, lines):
    folder = root / name
    for file in ["Cargo.toml", "Cargo.lock", "observer.rs"]:
        shutil.copyfile(ROOT / file, folder / file)
    shutil.copyfile(ROOT / f"{name}.rs", folder / "peer.rs")
    flags = "-Adead_code --check-cfg=cfg(reference)" + (" --cfg reference" if name == "original" else "")
    return run("cargo", "+1.97.1", "run", "--quiet", "--locked", "--offline", cwd=folder, input=lines,
               env={**os.environ, "CARGO_TARGET_DIR":str(folder/"target"), "RUSTFLAGS":flags})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("compiler", type=Path)
    parser.add_argument("original", type=Path)
    parser.add_argument("--package", type=Path, help="also check the already generated seven-member workspace")
    args = parser.parse_args()
    cases = list(corpus())
    lines = b"".join(json.dumps([op, raw.hex()]).encode()+b"\n" for op, _, raw, _ in cases)
    with tempfile.TemporaryDirectory(prefix="command-codec-") as tmp:
        root = Path(tmp)
        gen = root / "generated"
        run(args.compiler.resolve(), "build", "wire-protocol", "rust", MODEL/"shape.json", MODEL/"shape.rust.json",
            MODEL/"graph.json", MODEL/"rust.json", gen)
        (gen/"wire.rs").rename(gen/"codec.rs")
        if args.package:
            product = args.package / "marmish-native"
            if (args.package/"workspace.lock").read_bytes() != (MODEL.parent/"workspace-basic-commands/workspace.lock").read_bytes():
                raise ValueError("package lock differs from contract")
            digest = hashlib.sha256((gen/"codec.rs").read_bytes()).hexdigest()
            matches = [name for name, sha in json.loads((product/"package.lock").read_bytes())["generatedFiles"].items()
                       if sha == digest and hashlib.sha256((product/name).read_bytes()).hexdigest() == digest]
            if len(matches) != 1:
                raise ValueError("expected exactly one matching compiled component")
        candidate = observe(root, "generated", lines)
        reference = root / "original"
        reference.mkdir()
        def blob(file):
            return run("git", "-C", args.original, "show", f"{PIN}:crates/marmish-native/src/{file}.rs")
        (reference/"codec.rs").write_bytes(blob("command_wire"))
        # Compile the exact private helper referenced by the original codec.
        foundation = blob("foundation")
        start = foundation.index(b"pub(crate) fn valid_hex(")
        end = foundation.index(b"\n}", start) + 2
        (reference/"foundation.rs").write_bytes(foundation[start:end])
        original = observe(root, "original", lines)
        if original != candidate:
            raise ValueError("original and generated request observations differ")
        expected = [raw.hex() if accepted else None for _, _, raw, accepted in cases]
        if [json.loads(line) for line in original.splitlines()] != expected:
            raise ValueError("reference does not reproduce the fixed corpus")
        print(json.dumps(dict(original=PIN, cases=len(cases), accepted=sum(c[3] for c in cases),
                              observationsSha256=hashlib.sha256(original).hexdigest(),
                              wireSha256=hashlib.sha256((gen/"codec.rs").read_bytes()).hexdigest(),
                              compilerSha256=hashlib.sha256(args.compiler.read_bytes()).hexdigest()), indent=2))


if __name__ == "__main__":
    main()
