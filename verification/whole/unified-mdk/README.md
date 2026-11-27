# Sealed six-member workspace

The workspace manifest adds the standalone `marmish-mdk` package to the
five-member recovered report, fixed vault and journal workspace. All five
prior per-member lock records are preserved. The MDK qualification covers
the crate-root `AdapterError` and `VersionObservation` public data types.
Each host matched eight observation rows and sixteen negative Rust compile
checks against the original. The same build also passed both sets of
sixteen release-info cases, the 600 vault value rows plus the strict and
holdout probes, and the journal record oracle.

The receipts share compiler revision `d4c429e0d4e85eefa70ff3cdbda5a97b472b588e`,
companion input revision `ecba2fe0c85345344a5b89ec7abf5baa361593c8`, and
original revision `af54339d250290490d62d4475a77539b7e739071`.
The compiler runs under Rust 1.97.1. The generator and locked offline
workspace build run with macOS sandbox or Linux bubblewrap isolation, and
the gate verifies that first-party sources and oracle callers are denied
there. Independent callers run afterward against the generated workspace
libraries and original release libraries. The MDK witnesses exercise data
only.

From a clean companion checkout, with cached locked dependencies and the
original release binaries built for the native host, run:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin qualify-six-member -- \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision d4c429e0d4e85eefa70ff3cdbda5a97b472b588e \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

The Rust verifier builds each pinned framework revision in a separate subdirectory of `--compiler-target`. Downstream workspace releases use the same sealed build and common witnesses. Their callbacks check release-specific package locks before compilation and run additional native oracles after the shared witnesses pass.
