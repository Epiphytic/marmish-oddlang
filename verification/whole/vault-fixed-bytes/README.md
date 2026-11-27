# Vault fixed-width public value

The [vault package](../../../oddlang/portable/vault-fixed-value-package.json) adds the public `Secret` type to the six previously qualified value and error types. Its [portable contract](../../../oddlang/portable/vault-fixed-bytes-v1/graph.json) specifies an owned 32-octet value, checked construction, failure identity, borrowed read access, and erasure of the owner's storage on drop. Ordinary Rust code can call `Secret::from_bytes`, `Secret::from_slice` and `Secret::as_bytes`. The package uses the same locked zeroize provider as the original.

Build the package from the companion checkout with Oddlang `595fb7a55dc2016cfab70c5b131bf2f6acfe9983` and an offline Cargo cache:

```sh
oddlang build native-package rust oddlang/portable/vault-fixed-value-package.json /tmp/generated-vault
cargo +1.97.1 build --manifest-path /tmp/generated-vault/Cargo.toml --release --offline --locked
```

The verifier extracts the original checked storage shape and matches it against the generated library. It stages the pinned product inputs, denies access to both the original and companion source trees during generation and compilation, then links the original test module to the generated public API. The pinned original passes the same named test. Changing the width to 31 makes rustc reject the upstream assertion against a 32-byte array. The receipt records the constructor and error bindings that require independent qualification.

Set `ODDLANG` to the current Oddlang checkout containing the Rust capture tool. Set `FRAMEWORK` and `ORIGINAL` to the pinned checkouts and `RECEIPT` to the output path. Set `CACHE` to the build cache directory.

```sh
cargo +1.97.1 build --locked --offline --manifest-path "$ODDLANG/verification/provider-validation/Cargo.toml" --bin rust-capture
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/copy-manifest/Cargo.toml \
  --bin check-vault-fixed-bytes -- qualify "$FRAMEWORK" "$ORIGINAL" "$CACHE" "$RECEIPT" \
  --rust-capture "$ODDLANG/verification/provider-validation/target/debug/rust-capture"
```

The [Darwin ARM64 receipt](aarch64-apple-darwin.json) and [Linux ARM64 receipt](aarch64-unknown-linux-gnu.json) cover the default release profile on each host. Both qualify the public value and one upstream test. Keychain, Secret Service and the vault helper have separate obligations. The five-member product has its own pinned vault package.
