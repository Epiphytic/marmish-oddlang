# Canonical source-free native workspace

Build all seven members through this [workspace manifest](../../../oddlang/portable/workspace-v7-canonical-offline.json)
and the [qualified compiler](https://github.com/Epiphytic/oddlang/tree/8a20ea283d0d43da42f054a7c7774eb9e6a20da7).

```sh
oddlang build native-workspace rust \
  oddlang/portable/workspace-v7-canonical-offline.json "$OUTPUT"
cargo +1.97.1 build --manifest-path "$OUTPUT/Cargo.toml" --workspace --release --locked --offline
```

The composition starts with the converged public APIs, CLI and diagnostics.
It applies the independently verified terminal prefix graphs, adds the
crate-private pure release parser and public record declarations, and
selects the independently verified vault helper mode package. The original
Cargo lock, Cargo manifests and dependency edges remain the shared pins.
The gate verifies all emitted file digests and reconstructs the preceding
terminal native package lock from retained source pins.

The same-build verifier repeats the scoped original comparisons for
diagnostic output, terminal arguments, engine and path records, vault
values and helper mode, release metadata, the pure release parser, the
prototype reader and constant, journal types and MDK observations. The
Darwin public import audit compares the same frozen source inventory
against the assembled artifact. Qualification is limited to the recorded
contract roles and cases.

Run from clean checkouts with the original native release binaries and
cached offline dependencies available.

```sh
python3 -B verification/whole/canonical-workspace/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision 8a20ea283d0d43da42f054a7c7774eb9e6a20da7 \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

The [native macOS receipt](aarch64-apple-darwin.json) records 77 of 140
original import spellings, including six additions to the preceding
diagnostics and CLI workspace. It also records the terminal argument
cases, parser cases, and native result comparisons on the pinned lock.

The [native Linux receipt](aarch64-unknown-linux-gnu.json) uses the same
compiler revision, input tree and workspace lock. Both hosts matched 164
terminal argument cases and all 224 pure manifest rows, including 34
acceptances, and ran both unchanged original parser tests. The parser's
private import was rejected by the native compiler on both sides. Each
source-free build passed 31 explicit denial probes.
