# Verify copy Checkpoint

Check out the source and framework revisions in the [host receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached dependencies, run

```sh
python3 -B verification/whole/copy-checkpoint/verify.py /path/to/original /path/to/oddlang /tmp/checkpoint-receipt.json
```

The unchanged public Rust caller runs against the original and the generated package, and a separate Python encoder and decoder computes expectations. Cases cover every phase, present and absent final identities, checked timing and both directions of arithmetic overflow, nested Manifest errors, path bounds, every phase and option tag, checksum corruption, and recomputed checksum truncations. Repinning the checksum domain changes the exact output digest and invalidates the original footer.

## Checkpoint and Manifest linkage

The v5 Checkpoint declares which Manifest it embeds, its length limit, and two fields it reads from the embedded value. The package assembler can check those declarations against the actual Manifest graph and Rust overlay before emitting source.

With an Oddlang checkout containing revision `00af3dab60da87ef5a5f5b9ce84813c4051eed79`, run the following from the companion root. The verifier compiles the pinned compiler offline with Rust 1.96.1. It stages the portable stream package and uses Rust 1.97.1 for Cargo metadata. The original application checkout and binaries are unnecessary.

```sh
python3 -B verification/whole/copy-checkpoint/check_linkage.py /path/to/oddlang /tmp/checkpoint-linkage.json
```

The [receipt](linkage-aarch64-apple-darwin.json) records the published v5 package lock and a second successful projection with an explicit 16,384-byte limit on the Manifest codec. Five independently repinned controls change the codec limit, record identity, field bindings or enclosing slot budget. Each must fail with the expected diagnostic before the destination exists.
