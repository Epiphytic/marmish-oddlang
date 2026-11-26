# Native stream wire structural comparison

Use a clean Marmish checkout at `af54339d250290490d62d4475a77539b7e739071`, Rust 1.97.1 and the Oddlang JSON wire-schema generator. The verifier reproduces the archived graph and binding from the pinned input, then uses only those contract files for generation. It builds the original stream crate locked and offline and runs one Rust 2021 caller against both shapes.

```sh
python3 -B verification/whole/stream-wire/verify.py /path/to/oddlang /path/to/original /tmp/reference.json
```

The caller matched 17 public shape observations covering the binding, six request variants, six response variants and four terminal variants. Twelve original canonical request and response envelopes parsed and re-encoded byte for byte in the projected shape, and the original accepted the reproduced bytes. Eleven malformed controls exercised closed shape rejection, lowercase hexadecimal and bounded Base64-list diagnostic precedence.

Both sides bind the original public codec, block, enum and Copy Launch types. The selected cases leave the OpenCopy launch payload unexercised. The caller separately shows that empty arguments pass structural parsing but fail the original operation's size guard. Conditional operation guards, canonical admission of new input, time checks and separately generated external types remain open. [Pinned observation](../../../oddlang/portable/stream-wire-schema-v1/observation.json) · [Native receipt](reference.json)
