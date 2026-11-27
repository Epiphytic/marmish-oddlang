# Recover the native output-gap reason

The recovery command reads the pinned original `marmish-engine/src/model.rs` and a selector that maps Rust names to stable declaration identities. It emits the portable shared-declarations graph, a Rust naming and order overlay, and a digest ledger. The recovered graph and overlay agree with the [independently authored contract](../gap-range-reference/README.md). Their generated package source and component projection digest agree byte for byte.

Native Rust callers construct variants and check Clone, Debug, total equality, implicit ordinals, exact JSON conversion, and rejection of a Copy bound. The baseline reuses all 17 independent reason vectors. A real source mutation moves `StorageFailure` to the first declaration and gives it the symmetric wire spelling `Storage-Failure`. The compiler-confirmed original and mutated portable projection accept the new string, reject the old one and report the same changed order and diagnostics. No engine operations are invoked.

Generate the candidate from the graph, Rust overlay and pinned Cargo lock staged in a separate directory. The source digest is needed for recovery only. The source profile is a syntax candidate because native compilation independently establishes actual derive expansion and the crate-root public export.

```sh
python3 -B verification/whole/gap-reason-recovery/verify.py FRAMEWORK ORIGINAL RECEIPT \
  --framework-revision FRAMEWORK_REVISION --compiler-target COMPILER_TARGET \
  --reference-target REFERENCE_TARGET --mutation-target MUTATION_TARGET
```

To recover and project just the native package, run the Rust tool directly. The output directory must be new. It contains the recovered graph and overlay under `recovered/`, the staged package inputs under `portable-inputs/`, and the generated Rust crate under `product/`. The command prints the recovery and package digests as JSON.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin gap-reason-contract -- FRAMEWORK_CLI ORIGINAL/crates/marmish-engine/src/model.rs \
  oddlang/portable/engine-gap-reason-recovered-v1/selection.json NEW_DIRECTORY
```

The [recovery compiler](https://github.com/Epiphytic/oddlang/tree/d0b715eb05c5f150164aa89d5d80ae19f0da01ce) is pinned at `d0b715e` and the original at `af54339d250290490d62d4475a77539b7e739071`. The published receipts record the exact companion input, recovery digests, original library digests and generated file locks. [ARM64 macOS](aarch64-apple-darwin.json) · [ARM64 Linux](aarch64-unknown-linux-gnu.json)
