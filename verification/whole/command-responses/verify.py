"""Compare the response chunk type through isolated generated and upstream Rust consumers."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
REQUESTS = HERE.parent / "command-requests"
MODEL = HERE.parents[2] / "oddlang/portable/command-response-shapes-v1"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"


def compact(data):
    return json.dumps(data, separators=(",", ":")).encode()


def cases():
    def chunk(data, cursor):
        return compact({"bytes": base64.b64encode(data).decode(), "next_cursor": cursor})
    for data, cursor in [(b"", 0), (bytes(range(256)), 9007199254740993),
                         (bytes(16384), 2**64-1)]:
        value = chunk(data, cursor)
        yield value, value
    for value in [chunk(bytes(16385), 1), chunk(b"", 2**64), chunk(b"", -1),
                  b'{"bytes":"/x==","next_cursor":0}', b'{"bytes":null,"next_cursor":0}',
                  b'{"bytes":"","next_cursor":0,"unknown":true}', b'{"next_cursor":0}',
                  b'{"bytes":""}', b'{"bytes":"","bytes":"","next_cursor":0}']:
        yield value, None


def observe(folder, source, raw, reference):
    folder.mkdir()
    (folder / "codec.rs").write_bytes(source)
    for filename in ("Cargo.toml", "Cargo.lock", "observer.rs"):
        shutil.copyfile(REQUESTS / filename, folder / filename)
    shutil.copyfile(HERE / "adapter.rs", folder / "peer.rs")
    flags = "-Adead_code --check-cfg=cfg(reference)"
    if reference:
        prefix = f"{ORIGINAL}:crates/marmish-native/src/foundation.rs"
        blob = subprocess.check_output(["git", "-C", reference, "show", prefix])
        start = blob.index(b"pub(crate) fn valid_hex(")
        (folder / "foundation.rs").write_bytes(blob[start:blob.index(b"\n}", start)+2])
        flags += " --cfg reference"
    return subprocess.check_output(["cargo", "+1.97.1", "run", "--locked", "--offline", "--quiet"],
                                   cwd=folder, input=raw, env={**os.environ, "RUSTFLAGS": flags,
                                                             "CARGO_TARGET_DIR": str(folder/"target")})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("original", type=Path)
    args = parser.parse_args()
    corpus = list(cases())
    lines = b"".join(compact(["chunk", raw.hex()])+b"\n" for raw, _ in corpus)
    expected = [out.hex() if out is not None else None for _, out in corpus]
    with tempfile.TemporaryDirectory(prefix="response-chunk-") as tmp:
        root = Path(tmp)
        generated = root / "projection"
        subprocess.run([args.compiler.resolve(), "build", "wire-schema", "rust", MODEL/"graph.json",
                        MODEL/"rust.json", generated], check=True)
        source = (generated / "wire.rs").read_bytes()
        candidate = observe(root/"candidate", source, lines, None)
        blob = subprocess.check_output(["git", "-C", args.original, "show",
                                       f"{ORIGINAL}:crates/marmish-native/src/command_wire.rs"])
        reference = observe(root/"reference", blob, lines, args.original.resolve())
        if reference != candidate or [json.loads(line) for line in candidate.splitlines()] != expected:
            raise ValueError("response chunk observations differ")
    print(json.dumps({"referenceRevision": ORIGINAL, "cases": len(corpus), "accepted": sum(
        result is not None for result in expected), "observationsSha256": hashlib.sha256(candidate).hexdigest(),
        "generatedSourceSha256": hashlib.sha256(source).hexdigest()}, indent=2))


if __name__ == "__main__":
    main()
