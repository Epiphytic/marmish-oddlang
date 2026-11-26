"""Compare response data types through isolated generated and upstream Rust consumers."""
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


def chunks():
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


def cases():
    for raw, expected in chunks():
        yield "chunk", raw, expected
    for word in ("accepted", "existing", "another"):
        raw = compact(word)
        yield "disposition", raw, None if word == "another" else raw
    gap = dict(end=None, reason="uncertain_tail", start=9007199254740993)
    stream = dict(closed=True, gaps=[gap], produced=9007199254740993, retained=0)
    inp = dict(accepted=0, delivered=0, eof_sent=False, health="open", length=0)
    outcome = dict(kind="code", value=7)
    status = dict(admission="engine", authority_valid=True, cancel_requested=False,
                  engine_known=True, input=inp, invocation="55"*16, outcome=outcome,
                  state="exited", stderr=stream, stdout=stream, warning=None)
    for kind, document, nullable in [
        ("gap", gap, ("end",)), ("outcome", outcome, ("value",)),
        ("error", dict(code="unauthorized", detail=None), ("detail",)),
        ("status", status, ("outcome", "warning")),
    ]:
        raw = compact(document)
        yield kind, raw, raw
        yield kind, compact(dict(reversed(list(document.items())))), raw
        yield kind, compact(dict(document, unexpected=1)), None
        for key in nullable:
            value = dict(document)
            del value[key]
            yield kind, compact(value), None
    for value in [0, 255, 256, -1, None]:
        raw = compact(dict(kind="code", value=value))
        yield "outcome", raw, raw if value is None or 0 <= value <= 255 else None
    for end in (0, 2**53+1, 2**64-1, 2**64, -1):
        raw = compact(dict(gap, end=end))
        yield "gap", raw, raw if 0 <= end < 2**64 else None
    for field in ["admission", "state", "warning"]:
        yield "status", compact(dict(status, **{field:"unrecognized"})), None
    yield "status", compact(dict(status, invocation="AB"*16)), None
    yield "status", compact(dict(status, input=dict(inp, length=2**32))), None
    yield "error", compact(dict(code="unknown_error", detail=None)), None

    for field, values in {
        "admission": ("accepted", "dispatching", "engine"),
        "state": ("accepted", "launching", "running", "exited", "unknown_launch", "lost"),
        "warning": ("stdin_unconsumed", "input_failed", None),
        "outcome": (None, outcome),
    }.items():
        for value in values:
            raw = compact(dict(status, **{field: value}))
            yield "status", raw, raw
    for reason in ("limit", "storage_failure", "expired", "uncertain_tail"):
        raw = compact(dict(gap, reason=reason))
        yield "gap", raw, raw
    for code in ("invalid_request unauthorized role_disabled command_denied command_not_found "
                 "command_not_executable command_resolution_failed command_policy_error noncanonical_executable "
                 "invocation_conflict expired future_deferred future_excessive clock_behind_floor quota_exceeded "
                 "busy not_found result_expired recovery_uncertain input_failed storage_failure "
                 "transport_degraded authority_revoked").split():
        raw = compact(dict(code=code, detail="example"))
        yield "error", raw, raw
    for kind in ("code", "signal", "not_started", "launch_failed"):
        raw = compact(dict(outcome, kind=kind))
        yield "outcome", raw, raw
    for health in ("open", "eof_pending", "closed", "gap", "uncertain"):
        raw = compact(dict(status, input=dict(inp, health=health)))
        yield "status", raw, raw
    for length in (2**32-1, -1):
        raw = compact(dict(status, input=dict(inp, length=length)))
        yield "status", raw, raw if length >= 0 else None
    yield "status", compact(dict(status, input=dict(reversed(list(inp.items()))))), compact(status)
    for inv in ("55"*15, "55"*17):
        yield "status", compact(dict(status, invocation=inv)), None
    for start in (None, -1, 2**64):
        yield "gap", compact(dict(gap, start=start)), None
    yield "outcome", compact(dict(kind="code", value=7.0)), None
    yield "error", b'{"code":"busy","detail":null,"detail":null}', None


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
    lines = b"".join(compact([kind, raw.hex()])+b"\n" for kind, raw, _ in corpus)
    expected = [out.hex() if out is not None else None for _, _, out in corpus]
    with tempfile.TemporaryDirectory(prefix="response-shapes-") as tmp:
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
            raise ValueError("response structure observations differ")
    print(json.dumps({"referenceRevision": ORIGINAL, "cases": len(corpus), "accepted": sum(
        result is not None for result in expected), "observationsSha256": hashlib.sha256(candidate).hexdigest(),
        "generatedSourceSha256": hashlib.sha256(source).hexdigest()}, indent=2))


if __name__ == "__main__":
    main()
