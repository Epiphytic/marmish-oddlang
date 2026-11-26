# Verify copy helper context

Check out the original and framework revisions in [the host receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and registry dependencies cached, run

```sh
python3 -B verification/whole/copy-helper-context/verify.py /path/to/original /path/to/oddlang /tmp/helper-context-receipt.json
```

The generator and offline library build consume only staged graph, binding and Cargo lock inputs. On macOS they run with networking and original and companion reads denied. A separate unchanged Rust caller compiles against each library. An independent Python oracle checks exact bytes, decoded field values and Rust error diagnostics.

The corpus covers native construction bounds, all truncation points of ordinary and maximum-sized valid records, all 256 direction tags, invalid ASCII or UTF-8, oversized declared lengths, exact aggregate input limits, trailing bytes and simultaneous failures. The repinned field-order mutation must produce the independently predicted changed bytes. The receipt is scoped to the macOS ARM64 library API.
