# Verify copy Manifest

Check out both revisions from [the receipt](reference-aarch64-apple-darwin.json). With Rust 1.97.1 and dependency caches prepared, run from the companion root.

```sh
python3 -B verification/whole/copy-manifest/verify.py /path/to/original /path/to/oddlang /tmp/copy-manifest-receipt.json
```

The generator and offline package build consume pinned graph, bindings and Cargo lock stores. On macOS the verifier checks that the runner denies source reads from both repositories. An unchanged native caller and an independent Python oracle compare construction, exact bytes, namespace, canonical digests, complete decode results and ordered errors.

The cases include all 512 ordinary permission combinations, signed timestamps at both endpoints, nanos, path and offset boundaries, every tag byte for direction, flags and options, truncation and competing errors. A repinned mutant changes namespace order and defers the early size check. The receipt covers the macOS ARM64 library API.
