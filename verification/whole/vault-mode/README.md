# Public vault helper mode

The [portable choice](../../../oddlang/portable/vault-mode-v1/graph.json) contains two ordered cases and explicitly declares that equality is absent. Its [independent test obligations](../../../oddlang/portable/vault-mode-v1/obligations.json) require case order and copying. The Rust overlay supplies the native type and variant names. The [vault package](../../../oddlang/portable/vault-fixed-value-mode-package.json) exports the type through `marmish_vault::helper::AuthorizationMode` alongside the earlier checked and fixed-width values.

Using the Oddlang CLI built from `18ea58006de4482c0ebc61de9e800edc78eba987`, generate the package from the root of this companion repository:

```sh
out=$(mktemp -d)
oddlang build native-package rust oddlang/portable/vault-fixed-value-mode-package.json "$out/vault"
cmp "$out/vault/package.lock" oddlang/portable/vault-mode-v1/package.lock
cargo +1.97.1 build --locked --offline --release --manifest-path "$out/vault/Cargo.toml"
```

The verification script compiles the pinned Oddlang source, recovers the pure type from the pinned original helper module, then projects the package with the original source and caller hidden. A downstream Rust caller checks exhaustive matching, copying, cloning and discriminants. Seven independent compile checks require E0277 for missing capabilities. Reversing the original cases in a temporary checkout reverses their native ordinals in both the original and generated package. The verifier gives baseline and mutant original Cargo builds distinct target directories. The callers only inspect the public enum.

Run `python3 -B verify.py ORIGINAL ODDLANG COMPILER_CACHE REFERENCE_CACHE RECEIPT.json` from this directory. Use the pinned upstream Marmish commit `af54339d250290490d62d4475a77539b7e739071`. Keep the reference cache separate from other qualifiers.

The [macOS ARM64 receipt](aarch64-apple-darwin.json) and [Linux ARM64 receipt](aarch64-unknown-linux-gnu.json) record identical baseline and mutated graph and package lock hashes. Both hosts observed the same values and all seven rejected trait bounds. These measurements cover the vault package independently.
