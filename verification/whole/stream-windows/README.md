# Stream window qualification

Run the native generation, original comparison and mutation control from this checkout on macOS ARM64 with Rust 1.97.1 and locked registry dependencies cached.

```sh
python3 -B verification/whole/stream-windows/release.py \
  /path/to/pinned/marmish /path/to/pinned/oddlang /tmp/window-replay.json
cmp verification/whole/stream-windows/reference-aarch64-apple-darwin.json /tmp/window-replay.json
```

The release builds offline in a read-allowlisted process before it gains access to any original tests. The verifier compares all generated source hashes with the pinned package lock. It then embeds the *unchanged* five-test upstream module at its corresponding Rust privacy scope, with test-only imports. The ordinary downstream caller is compiled separately against each library. The digest of the unmodified release source is recorded before test insertion.

The frozen subset contains the 33 window traces from the public stream freeze, checked against their archived original results. The independent seeded input adds 96 traces and observes public state after each operation. Both implementations matched across 13,031 observations in total. Altering the error for a conflicting retained retry fails an unchanged upstream test. The receipt covers the named target and functions. [Frozen corpus source](https://github.com/Epiphytic/oddlang/tree/a635eec8e0f71d2dabc9aee28655e26494a7506f/examples/marmish/portable-oracle)
