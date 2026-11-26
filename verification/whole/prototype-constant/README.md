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

Run from a clean checkout with Rust 1.97.1, offline dependencies, and the
pinned upstream release binaries available.

```sh
python3 -B verification/whole/prototype-constant/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision c70bc9b3b39f6c4a23641679185dea5d0e5bc32b \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```
