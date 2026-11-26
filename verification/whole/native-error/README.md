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

### Build the v7 executables and native error API together

Use the [combined manifest](../../../oddlang/portable/workspace-v7-native-error.json)
with Oddlang `da0ea5dedd1adfa8513b7f3f8d536713958bcecb`. The native package
imports both reviewed components and depends on the generated stream member.
With the pinned dependencies available in the offline Cargo cache, run:

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v7-native-error.json /tmp/marmish-combined
cargo +1.97.1 build --manifest-path /tmp/marmish-combined/Cargo.toml --workspace --release --offline --locked
```

The replay stages only the manifest's graph, overlay and lock files. It builds
both release executables, compares their stdout, stderr and exit status with
the platform's published v7 observations and runs the unchanged native error
caller against a debug build of the same workspace. The replay uses the stored
original observations and requires no original Marmish checkout.

```sh
python3 -B verification/whole/native-error/combined.py /path/to/pinned/oddlang /tmp/marmish-combined-cache /tmp/combined.json
```

The [Darwin ARM64 combined receipt](darwin-arm64/combined.json) records 32 CLI
invocations and 32 caller observations. The full v7 regression run and Linux
ARM64 receipts cited in the workspace guide apply to the original v7 manifest.
