"""Freeze whole response admission and canonical output using the pinned upstream codec."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from verify import HERE, ORIGINAL, compact, observe


def encoded(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True).encode()


def cases():
    stream = dict(closed=True, gaps=[], produced=2, retained=2)
    inp = dict(accepted=0, delivered=0, eof_sent=True, health="closed", length=0)
    status = dict(admission="engine", authority_valid=True, cancel_requested=False, engine_known=True,
                  input=inp, invocation="55"*16, outcome=dict(kind="code", value=7), state="exited",
                  stderr=stream, stdout=stream, warning=None)
    chunk = dict(bytes="AP8=", next_cursor=2)
    body = dict(error=None, ok=True, request_sha256="77"*32, result=None)
    def frame(op, payload, **kw):
        envelope = dict(body=payload, client="11"*32, id="66"*16, op=op, protocol="marmish-command/1",
                        server="22"*32, session="44"*16, type="response")
        return encoded(dict(envelope, **kw))

    for op, result in {
        "submit": dict(disposition="accepted", status=status), "status": dict(status=status),
        "cancel": dict(status=status), "finish": dict(finished=True),
        "poll": dict(status=status, stderr=chunk, stdout=dict(bytes="Cg==", next_cursor=1))
    }.items():
        good = dict(body, result=result)
        raw = frame(op, good)
        yield f"{op}/success", raw, True
        yield f"{op}/trailing-newline", raw+b"\n", False
        yield f"{op}/duplicate-id", raw.replace(b'"id":', b'"id":"123","id":', 1), False
        yield f"{op}/same-peer", frame(op, good, server="11"*32), False
        yield f"{op}/wrong-message-type", frame(op, good, type="request"), False
        yield f"{op}/two-results", frame(op, dict(good, error=dict(code="busy", detail=None))), False
        yield f"{op}/missing-result", frame(op, body), False
        yield f"{op}/unknown-result-field", frame(op, dict(good, result=dict(result, other=True))), False
        err = dict(code="authority_revoked", detail=None)
        yield f"{op}/error", frame(op, dict(body, ok=False, error=err)), True
        yield f"{op}/bad-error", frame(op, dict(body, ok=False, error=dict(err, detail="other"))), False
        if op == "finish":
            yield "finish/false", frame(op, dict(body, result=dict(finished=False))), False
        if op == "poll":
            for length in (16384, 16385):
                half = length // 2
                def data(n):
                    return dict(bytes=base64.b64encode(bytes(n)).decode(), next_cursor=n)
                both = dict(result, stdout=data(half), stderr=data(length-half))
                yield f"poll/combined-{length}", frame(op, dict(body, result=both)), length == 16384
            for cursor in (2**53+1, 2**63-1, 2**63, 2**64-1):
                item = dict(result, stdout=dict(chunk, next_cursor=cursor))
                yield f"poll/cursor-{cursor}", frame(op, dict(body, result=item)), cursor < 2**63
            for value in (0, 1, 127, 128):
                measured = dict(status, outcome=dict(kind="signal", value=value))
                yield f"poll/signal-{value}", frame(op, dict(body, result=dict(result, status=measured))), 1 <= value <= 127


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    args = parser.parse_args()
    caseset = list(cases())
    if len({name for name, _, _ in caseset}) != len(caseset):
        raise ValueError("duplicate case identity")
    payload = b"".join(compact(["response", raw.hex()])+b"\n" for _, raw, _ in caseset)
    source = subprocess.check_output(["git", "-C", args.original, "show",
                                      f"{ORIGINAL}:crates/marmish-native/src/command_wire.rs"])
    with tempfile.TemporaryDirectory(prefix="response-envelopes-") as tmp:
        results = observe(Path(tmp)/"original", source, payload, args.original.resolve(),
                          HERE/"reference_envelope.rs").splitlines()
    rows = {}
    for (name, raw, accepted), actual in zip(caseset, results, strict=True):
        if json.loads(actual) != (raw.hex() if accepted else None):
            raise ValueError(f"reference disagrees on {name}")
        rows[name] = {"accepted":accepted, "wireBytes":len(raw), "wireSha256":hashlib.sha256(raw).hexdigest()}
    print(json.dumps({"referenceRevision":ORIGINAL, "referenceSourceSha256":hashlib.sha256(source).hexdigest(),
                      "cases":rows}, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
