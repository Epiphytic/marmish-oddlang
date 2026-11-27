# Engine errors and native identity in the sealed workspace

The seven-member release adds the public `marmish_engine` error family
and the existing `marmish_native::identity::BackendChoice` value. Only
the engine and native member lock records change. All sibling edges and
the root Cargo.lock are preserved. The process-limit projection refers
to the shared `WorkerUnavailable` variant by stable identity and Rust
type path. The legacy standalone process-limit projection retains its
previous package lock.

The independent Rust caller constructs all 26 engine variants and
exhaustively matches the three reason enums. It compares 34 error
values and eight reason values, including zero and maximum credits and
offsets, Display, Debug, pretty Debug, equality, source and the generic
Result alias. Thirty-eight compile negatives check undeclared traits,
cross-reason typing, field widths, exhaustive matching and private module
visibility. The unchanged native identity caller covers its three cases,
ordinals and twelve compile negatives. If Cargo reports several Serde
artifacts, the verifier compiles a journal Serialize witness against each
and requires exactly one compatible trait identity for each build.

The macOS source mutation swaps two reason ordinals, changes the Busy
diagnostic and swaps the named backpressure fields. The same caller
observes the three changes in the compiled upstream source and in its
recovered, source-free projection. Neither caller runs an engine
operation. The earlier release-info, vault, journal, MDK and buffered
reader witnesses run against the same sealed build.

From a clean checkout with the pinned original release binaries and
cached dependencies, run:

```sh
cargo +1.96.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin qualify-six-member -- --public-types errors \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision bbdc69e026ee0ae0c50bd9884fa4b6eaad8040a1 \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

The two sealed receipts pin companion input
`ba1057a6c1ad5d483b96d7e28977876dbd4acc76` and original
`af54339d250290490d62d4475a77539b7e739071`. Both use native Rust 1.97.1
and deny the first-party sources and test callers during generation and
build.

The [macOS ARM64 callback replay](rust-buffered-callback-aarch64-apple-darwin.json) reran the complete seven-member qualification with the Rust private-reader oracle. Its four buffered cases, 26 engine-error variants, 38 negative Rust diagnostics and native backend-choice observations matched the archived receipt.

Check the portable descriptors from the companion repository root:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin workspace-product -- compose-api engine-public-error oddlang/portable --check
```

Omit `--check` to generate the descriptors.


Replay the source mutation with the built Oddlang CLI and an independent Cargo target directory:

```sh
cargo +1.97.1 run --quiet --locked --offline \
  --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin engine-mutation -- \
  errors "$ODDLANG_CLI" "$MARMISH_CHECKOUT" "$MUTANT_TARGET" "$RECEIPT" \
  --framework-revision "$ODDLANG_REVISION"
```
