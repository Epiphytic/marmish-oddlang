# Native stream block interoperability

Run the driver from the companion root on macOS ARM64 with Rust 1.97.1 and the locked Cargo dependencies cached.

```sh
python3 -B verification/whole/stream-block/verify.py /path/to/marmish \
  /path/to/package-oddlang /path/to/recovery-oddlang /tmp/stream-codec-receipt.json
cmp /tmp/stream-codec-receipt.json verification/whole/stream-block/reference-aarch64-apple-darwin.json
```

The driver first assembles and builds a fresh package from the manifest, two graphs, two native bindings and Cargo.lock. The library has no upstream source dependency. Reference compilation, recovering the shared graph, and the independently linked comparison callers are separate verification steps. This driver stages inputs and inspects pins. Its host filesystem is accessible during the build.

All five upstream codec tests run unchanged on both crates. A native caller observes the six constants and nine public errors, makes 192 encode requests and checks 82 decoding cases including malformed, concatenated, truncated and over-bound inputs. Both peers read 160 original/generated records each, comparing payloads, offsets, checksums and validation results. A Python zlib stream supplies an independent member encoding. A mutation narrows the offset range by one and must refuse the original last valid byte. These checks apply to the codec module on the recorded native host.
