# Native error API

This replay covers `marmish_native::error::{Error, Result}` and the equivalent
crate-root reexports. A single independent Rust caller exhaustively matches the
public enum and both dependency enums. It measures plain and padded/precision
Display, Debug, source, typed payloads, I/O conversion through `?`, and the
single generic success type. The 32 rows include every one of the 14 copy and
10 stream categories.

The I/O provider message stays out of Display. Rust's public `Error::source()`
retains the underlying `std::io::Error`, and Debug reflects the derived
representation. The API accepts text payloads for categories which explicitly
display their inputs. Assembly and qualification here are limited to this
public error module and do not certify redaction at its callers.

The independent mutation changes only the copy quota label. The verifier
commits that change in a scratch checkout, recovers the revised category from
Rust, checks the exact graph delta, builds another generated workspace and
compares both native caller outputs. The baseline uses the reviewed v6 packages
with a pinned sibling dependency from native to stream. The only inputs for
the added generated Rust component are its graph and Rust overlay.

Run on native Darwin ARM64 with Cargo and rustc 1.97.1, a clean original checkout
at the recorded revision, pinned Cargo dependencies available offline, and an
Oddlang executable built from the framework revision in the receipt.

```sh
RUSTUP_TOOLCHAIN=1.97.1 python3 -B verification/whole/native-error/verify.py \
  /path/to/oddlang /path/to/original /tmp/marmish-native-error-evidence \
  --original-target /tmp/marmish-native-original-target \
  --generated-target /tmp/marmish-native-generated-target \
  --framework-revision FRAMEWORK_SHA
```

Use a fresh output directory. The target directories may be reused. The
published v6 manifest and release receipts retain their existing pins.
