# Retention recovered from Rust structure

This receipt qualifies the optional unsigned `Retention` interval in original
Marmish core `af54339d250290490d62d4475a77539b7e739071` on macOS ARM64 with Rust
1.97.1. The selection identifies the source module and binds stable declaration
identities. The extractor derives the width, interval, disabled state, default,
field accessor and selected error from closed Rust structure. It checks the
derived result through an independently compiled native API observer.

The baseline graph is byte-identical to the existing portable core
configuration. A separate local checkout changes the original Rust maximum
from 365 days to seven days and changes the corresponding diagnostic text.
The graph and both native implementations then reject 31,536,000 seconds.
The original and generated downstream caller transcripts agree within each
revision and differ across the source change.

The verifier assembles the native core candidate from graph inputs and binding
overlays, then copies in the pinned unchanged `contracts.rs`. It runs all seven
tests on the original and generated packages in each revision. The archive
includes checksums for that test file, the independent caller and the generated
package locks. Its graph and receipt contain no upstream function bodies.

Run the reproduction from a source checkout of the companion:

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin retention-structure -- \
  ODDLANG ORIGINAL NEW_OUTPUT --framework-revision FRAMEWORK_REVISION
```

Use the extractor revision recorded in the receipt. The structural matcher
covers one conventional public-reexported optional unsigned tuple newtype
and the closed forms documented in the framework plan. Additional guard
effects, arbitrary macros, type aliases, and general Rust behavior remain
outside this extraction.

The Rust verifier reproduced all seven archived files on Darwin ARM64, including
the baseline and seven-day caller transcripts and the receipt with SHA-256
`964b84660a8c31a96114b774381c2cf873a94d6e425862fb0408b20a7133c06f`.
