# Source-derived Start and Manifest fragment encoders

The pinned compiler observes the direct checked append used by `Start` and the checked fresh record encoder called by `Manifest`. It derives 25 top-level writes across the two records. That includes optional mode and the signed seconds and unsigned nanoseconds inside optional modification time. The baseline layouts agree with their released portable graphs.

The verifier copies the pinned upstream tree and swaps the client and server writes separately in each fragment writer. Both recovered layouts change. Independent native expectations check four distinct valid output rows and five exact rejection rows across each of the two upstream and two regenerated libraries. The generated packages deny both upstream source trees and compiler observations.

Run from this repository with upstream revision `af54339d250290490d62d4475a77539b7e739071` and the [recorded framework revision](reference-aarch64-apple-darwin.json). Rust 1.97.1 with the host's `rustc-dev` component is required.

Set `ODDLANG` to the Oddlang checkout containing the Rust capture tool and `TOOLS` to an absolute build directory. The recovery compiler is built separately from the framework revision pinned in the reference receipt.

```sh
cargo +1.97.1 build --locked --offline --bin rust-capture \
  --manifest-path "$ODDLANG/verification/provider-validation/Cargo.toml" --target-dir "$TOOLS"
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/copy-checkpoint/Cargo.toml \
  --bin check-copy-frame-fragments -- /path/to/upstream /path/to/pinned-oddlang \
  "$TOOLS/debug/rust-capture" /tmp/source-copy-frame-fragments.json
```

The macOS ARM64 [receipt](reference-aarch64-apple-darwin.json) covers the frame-to-validator delegation and encoder writes. The [dependency report](../../../oddlang/portable/source-copy-frame-fragments-v1/boundary-qualification.json) links independent validation and decode evidence and names the validator precedence cases covered here. The compiler result leaves validator and decoder bodies outside the recovered semantics.
