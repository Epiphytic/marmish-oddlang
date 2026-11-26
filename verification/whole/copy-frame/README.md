# Verify the complete copy protocol library

Check out the original and framework at the revisions in the [macOS ARM64 receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached dependencies, run

```sh
python3 -B verification/whole/copy-frame/verify.py /path/to/original /path/to/oddlang /tmp/copy-frame-receipt.json
```

The verifier stages only pinned graph, binding and Cargo lock inputs for the generator and offline build. macOS checks that reads from both repositories are denied. After the generated library is compiled, a separate native consumer receives the exact unchanged upstream test module and the additional witness.

Both libraries pass all 11 upstream tests and nine additional tests for canonical payloads, public constants and signatures, directional error precedence, inline record errors, bounds and decoder poisoning. The complete standalone Manifest and Checkpoint oracle corpora also run on the composed package. A repinned mutant reverses Data encode precedence and defers the decode offset check. The two named exact-error tests must fail with the expected opposing errors in the mutant.

See the [structural reconciliation and remaining gaps](structural-gap.md).
