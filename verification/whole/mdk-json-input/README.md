# MDK private JSON test seam

The pinned upstream case `tests::synthetic_json_and_ndjson_are_bounded_and_structural_only` passes unchanged in upstream and against the generated private helpers on native macOS and Linux ARM64. The generated package keeps the prior public MDK data components and adds a private, test-only parser for supplied bytes and buffered input.

The independent observer found a 65,536-byte limit for a standalone value and for each LF-terminated line, including the LF. It also found a 200-record limit, last-value handling for duplicate members, and byte-limit, record-limit, then parse guard order. Mutating the pinned source at its byte and record limits changed the recovered bounds to 32,768 bytes and 181 records. Both generated packages reproduced their corresponding original observations. Four source-denial probes ran before projection. The unchanged upstream test and independent observer were added to disposable products after the sealed release builds.

This evidence covers one named case and supplied-byte metadata parsing only. It leaves the operational adapter and other upstream tests outside this release.

Run from a clean checkout with the pinned original and compiler revisions. The runner builds offline with Rust 1.97.1. Use separate writable target directories for the compiler and upstream oracle.

```sh
python3 -B verification/whole/mdk-json-input/verify.py \
  /path/to/oddlang /tmp/mdk-json-compiler-target \
  /path/to/original /tmp/mdk-json-original-target /tmp/mdk-json-receipt.json \
  --framework-revision 1d3b45fb79f775a445ae874c06ed89c818b1f697
```
