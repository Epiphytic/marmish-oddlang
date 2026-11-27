# Native stream block interoperability

Run the driver from the companion root on macOS ARM64 or native Linux ARM64 with Rust 1.97.1 and the locked Cargo dependencies cached.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/stream-wire/Cargo.toml --bin replay-stream-block -- /path/to/marmish \
  /path/to/package-oddlang /path/to/recovery-oddlang /tmp/stream-codec-receipt.json
host=$(rustc +1.97.1 -vV | sed -n 's/^host: //p')
cmp /tmp/stream-codec-receipt.json "verification/whole/stream-block/reference-${host}.json"
```

The driver first assembles and builds a fresh package from the manifest, two graphs, two native bindings and Cargo.lock. The library has no upstream source dependency. Reference compilation, recovering the shared graph, and the independently linked comparison callers are separate verification steps. This driver stages inputs and inspects pins. Its host filesystem is accessible during the build.

All five upstream codec tests run unchanged on both crates. A native caller observes the six constants and nine public errors, makes 192 encode requests and checks 82 decoding cases including malformed, concatenated, truncated and over-bound inputs. Both peers read 160 original/generated records each, comparing payloads, offsets, checksums and validation results. The Rust verifier preserves the original Python-generated zlib member as a fixture and checks both peers’ compressed replies with the system zlib decoder. A mutation narrows the offset range by one and must refuse the original last valid byte. These checks apply to the codec module on the two recorded native hosts.

## Formatted error text

The earlier macOS receipt covered default display. At generator `274d8e2`, a caller's width and precision settings could pad or truncate the payload embedded in `Error::Invalid`. The upstream implementation always formats that payload with its default display. Generator `828fc25` preserves that behavior. The declaration probe compares the original and generated outputs for three payloads, including empty and Unicode text, with five formatting patterns. It independently checks that all 15 texts contain the complete payload after the exact upstream prefix.
