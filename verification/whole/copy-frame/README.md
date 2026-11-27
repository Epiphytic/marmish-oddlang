# Verify the complete copy protocol library

Check out the original and framework at the revisions in the [macOS ARM64 receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached dependencies, run

```sh
python3 -B verification/whole/copy-frame/verify.py /path/to/original /path/to/oddlang /tmp/copy-frame-receipt.json
```

The verifier stages only pinned graph, binding and Cargo lock inputs for the generator and offline build. macOS checks that reads from both repositories are denied. After the generated library is compiled, a separate native consumer receives the exact unchanged upstream test module and the additional witness.

Both libraries pass all 11 upstream tests and nine additional tests for canonical payloads, public constants and signatures, directional error precedence, inline record errors, bounds and decoder poisoning. The complete standalone Manifest and Checkpoint oracle corpora also run on the composed package. A repinned mutant reverses Data encode precedence and defers the decode offset check. The two named exact-error tests must fail with the expected opposing errors in the mutant.

See the [structural reconciliation and remaining gaps](structural-gap.md).

## Data frame pipe interoperability

Build a small stdin/stdout driver separately against the pinned original and the regenerated seven-member command-policy workspace. The checker feeds the original encoder into the generated decoder and then swaps them. For each direction it compares the live frame bytes with an independent encoding and compares the decoded payload with its input. The corpus spans every byte value, full and partial blocks and 1,260 consecutive offsets. Each decoder also receives a truncated final frame and a noncontiguous starting offset. This checks the local copy Data codec boundary. [macOS ARM64 receipt](pipe-aarch64-apple-darwin.json)

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin check-copy-frame-pipe -- /path/to/pinned-original /path/to/generated-command-policy-workspace
```
