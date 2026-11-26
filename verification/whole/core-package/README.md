# Whole core package qualification

The same Rust 2021 caller runs against the pinned original `marmish-core`
and a fresh library built from the [package manifest](../../../oddlang/portable/core-package.json).
The verifier assembles two copies and checks byte identity and every generated
source hash against the [package lock](../../../oddlang/portable/core-package-v1/package.lock).
It then builds the generated release offline. macOS Seatbelt denies network
access and reads from the original, companion and framework checkouts. The
verifier checks the read restrictions and denies a connection to a local
listener before assembly.

After the generated release build, the verifier copies the unchanged
upstream integration tests and the independent caller into the test crate.
The projected test process retains the source restrictions and cannot read
the temporary reference source, target directory or reference transcript.
Both implementations run the full upstream test command, pass all seven
tests and produce byte-identical caller transcripts. The caller observes relay rule order, DNS and IP prefix
boundaries, deduplication, retention, exact frame bytes, object and tuple
inputs, version and Base64 precedence, configured bounds and deterministic
malformed inputs. It checks the native error variant ordinals and exact
diagnostics.

Four semantic mutations change the retention upper bound, reorder raw URL
guards, change the frame version and reorder named frame errors. The
differential checks reject each mutation at an identified observation. A
graph change without its manifest digest update is rejected before assembly.

Run this on macOS ARM64 with clean original and framework checkouts at the
revisions in the [native qualification receipt](reference-aarch64-apple-darwin.json),
Rust 1.97.1 and the locked Cargo dependencies cached locally.

```sh
python3 -B verification/whole/core-package/verify.py \
  /path/to/original /path/to/pinned-oddlang /tmp/core-package-replay.json
cmp /tmp/core-package-replay.json verification/whole/core-package/reference-aarch64-apple-darwin.json
```

The separate public API witness compares compiler observations of the two
Rust crates. It records root-reexport names, unit variant order, private
tuple arity, method and constant types, borrowing and trait identities.
The original and projected witnesses were both identical to the archived
JSON at SHA-256 `4ce9a0c5153a3f78c2e6adc1163b4b6235e28970dc41c887acf5827b24e6ae5f`.
The comparison used framework revision
`79175c05928dc234d8f97f428472c65386e9f096`. [Public API witness](public-api-aarch64-apple-darwin.json)

Run pinned Rust 1.97.1 release rustdoc on the original and projected libraries
with `--locked --offline --target aarch64-apple-darwin --lib -p marmish-core`
and forward `-Z unstable-options --output-format json` to rustdoc. Set
`RUSTC_BOOTSTRAP=1` for that documented JSON observer. Send each JSON document
to the pinned `oddlang inspect native-value-api FILE.json` and compare the
two normalized outputs. Runtime checks provide the observed constant values,
messages and behavior separately. [Framework API witness documentation](https://github.com/Epiphytic/oddlang/blob/79175c05928dc234d8f97f428472c65386e9f096/verification/rust-recovery/README.md)
