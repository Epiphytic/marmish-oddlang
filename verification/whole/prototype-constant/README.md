# Prototype public MAX_LINE

The successor keeps the seven-member workspace and extends only
`marmish-prototype` with the crate-root export
`marmish_prototype::MAX_LINE: usize`. It retains the public value
`131072` and the recovered portable expression `128 × 1024`.
All six sibling lock rows, the Cargo lock, and the existing private reader
files are compared against the predecessor. The private reader's four
selected native cases run again. This projection covers the constant and
the previously qualified reader. Bridge and serve remain open obligations.

The product build stages only the portable graph, overlay and pinned
assembly inputs. The offline sandbox denies the actual source and the
unchanged downstream caller. After the build, the caller compiles using
the same public import and const-context check against the pinned original
and the generated crate, then prints the value. A rustdoc replay binds the
actual upstream public integer type to the checked expression, and a native
compiler run after the isolated `128 → 129` source mutation recovers
`132096` with the same overlay.

Build a fresh output directory with the pinned compiler from
[Oddlang](https://github.com/Epiphytic/oddlang/tree/c70bc9b3b39f6c4a23641679185dea5d0e5bc32b).

```sh
oddlang build native-workspace rust \
  oddlang/portable/workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit.json \
  "$OUTPUT"
cargo +1.97.1 build --manifest-path "$OUTPUT/Cargo.toml" --workspace --release --locked --offline
```

Run from a clean checkout with Rust 1.97.1, offline dependencies, and the
pinned upstream release binaries available.

```sh
python3 -B verification/whole/prototype-constant/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision c70bc9b3b39f6c4a23641679185dea5d0e5bc32b \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

[ARM64 macOS sealed receipt](aarch64-apple-darwin.json) · [ARM64 Linux sealed receipt](aarch64-unknown-linux-gnu.json).

The [macOS ARM64 callback replay](rust-buffered-callback-aarch64-apple-darwin.json) rebuilt the seven-member workspace and exercised the Rust private-reader oracle on its unchanged generated leaf. The original and generated native callers each printed `131072`, and the source-recovery mutation and all four reader tests reproduced their archived results.

Check the portable descriptors from the companion repository root:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin workspace-product -- compose-api prototype-constant oddlang/portable --check
```

Omit `--check` to generate the descriptors.
