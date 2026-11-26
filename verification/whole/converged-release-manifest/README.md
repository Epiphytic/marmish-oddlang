# Release manifest in the converged native workspace

This source-free workspace places the qualified pure parser inside
`marmish_native::native_manifest` alongside the existing native error
domain. Its Rust overlay names the selected crate-private records,
`version` predicate and ordered `TARGETS` array. The parser uses the
same portable graph as the standalone release. The other six package
locks, Cargo lock, native sibling edges and preceding native generated
files are checked against the converged public API release.

The qualifier independently inspects the original and emitted record
types with Rustdoc, compiles a downstream import that must fail on native
privacy, and compares 224 full results against the native original.
It attaches the original pure tests `strict_vectors` and
`shared_release_version_vectors` unchanged, together with their pinned
vector include, only after source-isolated generation and release build.
The tests execute inside the generated native module. Inherited release
report, journal, vault, MDK, engine error, identity and prototype reader
observations replay in the same workspace.

This phase covers only pure bytes-to-metadata parsing, formatters and
the two supporting pure declarations. Installer operations, asset I/O,
and CLI dispatch are separate qualifications.

Build the [workspace manifest](../../../oddlang/portable/workspace-v7-converged-release-manifest.json)
using the tested [Oddlang compiler](https://github.com/Epiphytic/oddlang/tree/da1b0033ae9345c2d8288b4164d4fd1dbf5bcade).

```sh
oddlang build native-workspace rust \
  oddlang/portable/workspace-v7-converged-release-manifest.json "$OUTPUT"
cargo +1.97.1 build --manifest-path "$OUTPUT/Cargo.toml" --workspace --release --locked --offline
```

Run the sealed native qualifier from a clean checkout with available
offline dependencies and the pinned original release binaries.

```sh
python3 -B verification/whole/converged-release-manifest/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision da1b0033ae9345c2d8288b4164d4fd1dbf5bcade \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

Add `--raw-results "$NATIVE_ROWS"` to retain the full validated native
result stream alongside the receipt.
