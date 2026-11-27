# Verify copy Checkpoint

Check out the source and framework revisions in the [host receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached dependencies, run

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/copy-checkpoint/Cargo.toml --bin copy-checkpoint-replay -- /path/to/original /path/to/oddlang /tmp/checkpoint-receipt.json
```

The unchanged public Rust caller runs against the original and the generated package, and the verifier’s independent Rust encoder and decoder compute expectations. The request-order test pins the entire original JSON-lines corpus. Cases cover every phase, present and absent final identities, checked timing and both directions of arithmetic overflow, nested Manifest errors, path bounds, every phase and option tag, checksum corruption, and recomputed checksum truncations. Repinning the checksum domain changes the exact output digest and invalidates the original footer.

## Checkpoint and Manifest linkage

The v5 Checkpoint declares which Manifest it embeds, its length limit, and two fields it reads from the embedded value. The package assembler can check those declarations against the actual Manifest graph and Rust overlay before emitting source.

With an Oddlang checkout containing revision `70821c898cabe15a5d0483dc9425e51aae883932`, run the following from the companion root. The verifier compiles the pinned compiler offline with Rust 1.96.1. It stages the portable stream package and uses Rust 1.97.1 for Cargo metadata. The original application checkout and binaries are unnecessary.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/copy-checkpoint/Cargo.toml --bin check-linkage -- /path/to/oddlang /tmp/checkpoint-linkage.json
```

The [receipt](linkage-aarch64-apple-darwin.json) records the published v5 package lock and a second successful projection with an explicit 16,384-byte limit on the Manifest codec. Five independently repinned controls change the codec limit, record identity, field bindings or enclosing slot budget. Each must fail with the expected diagnostic before the destination exists.

The [native verifier replay](rust-replay-macos-arm64.json) reproduced both archived macOS ARM64 receipts byte for byte. The independent oracle and both native callers agreed on 290 constructions and 1,827 decodes, of which 1,809 were rejected. The linkage checker reproduced both successful package-lock digests and the five archived compiler diagnostics.
