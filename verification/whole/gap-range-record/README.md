# Standalone MissingRange data contract

The [candidate package](../../../oddlang/portable/engine-gap-range-package.json) adds a public `marmish_engine::MissingRange` record to the verified MissingReason package. Its ordered fields are `u64`, `Option<u64>`, and the existing public choice. The default workspace is unchanged.

The Rust record decodes JSON objects and positional triples. Unknown object keys are admitted and discarded. An absent or null `end` decodes to an empty option and encodes as null. The bounds may appear in either order. The projected choice retains its original type identity and wire spellings.

Native [macOS ARM64](aarch64-apple-darwin.json) and [Linux ARM64](aarch64-unknown-linux-gnu.json) replays used the same published package lock `884180d21d079c49a7ca0d045b2f56d67700d1666012a92d7b8310f1bb2fe109`. Each replay compared the unchanged 35-row native caller against the previously captured exact original reference. The source mutation added only `#[serde(deny_unknown_fields)]` to the pinned record, built the original mutant afresh and changed exactly one input outcome. The graph mutation independently reproduced that rejection and all 34 unchanged outcomes. A separate native caller checked field types, direct construction, Clone, Eq and Debug on the original mutant and both generated records.

The packaging gate denies reads of original and mutant source and the two callers before building the standalone library. The test lane uses inert data and supplies no engine operations or deployment changes.

Replay with clean pinned checkouts, Rust 1.96.1 for the compiler, Rust 1.97.1 for the native callers and populated offline Cargo caches.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin range-contract-inputs -- qualify-record \
  "$ODDLANG" "$COMPILER_CACHE" "$ORIGINAL" "$ORIGINAL_TARGET" /tmp/gap-range-record.json \
  --framework-revision a5167dea515ee6196c0cd312fe71c2c4444a0789
```
