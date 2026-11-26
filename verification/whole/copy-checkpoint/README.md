# Verify copy Checkpoint

Check out the source and framework revisions in the [host receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached dependencies, run

```sh
python3 -B verification/whole/copy-checkpoint/verify.py /path/to/original /path/to/oddlang /tmp/checkpoint-receipt.json
```

The unchanged public Rust caller runs against the original and the generated package, and a separate Python encoder and decoder computes expectations. Cases cover every phase, present and absent final identities, checked timing and both directions of arithmetic overflow, nested Manifest errors, path bounds, every phase and option tag, checksum corruption, and recomputed checksum truncations. Repinning the checksum domain changes the exact output digest and invalidates the original footer.
