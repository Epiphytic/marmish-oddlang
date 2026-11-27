# Verify copy Manifest

Check out both revisions from [the receipt](reference-aarch64-apple-darwin.json). With Rust 1.97.1 and dependency caches prepared, run from the companion root.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/copy-manifest/Cargo.toml --bin copy-manifest-replay -- /path/to/original /path/to/oddlang /tmp/copy-manifest-receipt.json
```

The generator and offline package build consume pinned graph, bindings and Cargo lock stores. On macOS the verifier checks that the runner denies source reads from both repositories. An unchanged native caller and an independent Rust oracle compare construction, exact bytes, namespace, canonical digests, complete decode results and ordered errors.

The cases include all 512 ordinary permission combinations, signed timestamps at both endpoints, nanos, path and offset boundaries, every tag byte for direction, flags and options, truncation and competing errors. A repinned mutant changes namespace order and defers the early size check. The receipt covers the macOS ARM64 library API.

The [Linux ARM64 receipt](reference-aarch64-unknown-linux-gnu.json) records the same package lock, caller and observations as macOS, including the repinned mutation. The Linux run used the offline dependency cache and the source-denied bubblewrap build.

The [Rust replay](rust-replay-macos-arm64.json) reproduced the archived macOS receipt byte for byte. The regression test also compares the complete ordered request array and independent expected outcomes with the original verifier.
