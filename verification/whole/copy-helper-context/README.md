# Verify copy helper context

Check out the original and framework revisions in [the host receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and registry dependencies cached, run

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/copy-manifest/Cargo.toml --bin check-copy-helper-context -- /path/to/original /path/to/oddlang /tmp/helper-context-receipt.json
```

The generator and offline library build consume only staged graph, binding and Cargo lock inputs. On macOS they run with networking and original and companion reads denied. A separate unchanged Rust caller compiles against each library. An independent Rust oracle checks exact bytes, decoded field values and Rust error diagnostics.

The corpus covers native construction bounds, all truncation points of ordinary and maximum-sized valid records, all 256 direction tags, invalid ASCII or UTF-8, oversized declared lengths, exact aggregate input limits, trailing bytes and simultaneous failures. The repinned field-order mutation must produce the independently predicted changed bytes. The receipts cover the native library API on macOS and Linux ARM64.

On Linux ARM64, provision Rust 1.97.1, bubblewrap, setpriv and the registries for the pinned locks. The build runs in a separate network namespace with zero effective capabilities and an allowlist of inputs. The original and companion checkouts are inaccessible to the generator and native build. Run the same verifier and compare with the [Linux ARM64 receipt](reference-aarch64-unknown-linux-gnu.json).
