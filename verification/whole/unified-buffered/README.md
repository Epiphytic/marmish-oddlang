# Sealed seven-member workspace

Both native receipts describe one generated Rust workspace containing all
seven original package names. Each member implements the subset below.
The build validates physical package membership and generated file pins.
The observations in the receipts qualify the narrower selected behavior.

| Member | Included contract surface |
| --- | --- |
| `marmish-core` | Retention values, relay endpoints, and JSON envelopes |
| `marmish-stream` | Shared values, integrity blocks, selected Copy records and framing, wire records and operations, and sequence journal |
| `marmish-native` | Selected CLI, local status and file-backed foreground/profile paths, build report, errors, and journal observation records |
| `marmish-vault` | Shared errors and checked file-key values |
| `marmish-engine` | Process-limit observations and startup contract |
| `marmish-mdk` | `AdapterError` and `VersionObservation` |
| `marmish-prototype` | Private in-memory buffered reading, refill and consumed-span erasure |

The seven-member manifest preserves all six existing member lock rows.
The root lock adds one prototype package row and uses the existing
`zeroize` provider identity. The recorded comparison replays 32 release-info
cases, 600 vault value rows and the strict and holdout tests, three journal
records, and eight MDK observation rows with sixteen negative compile checks.
Four private buffer cases run after the same sealed build. They include the
unchanged original consumed-span test and three independent holdouts keyed
to the original host receipt. The private reader qualification is limited
to local memory and exposes no bridge, serve or worker behavior.

The generator and locked offline build run with macOS sandbox or Linux
bubblewrap isolation. The receipts record twelve denied source and caller
probes. The test harness admits comparison code only after generation and
the release build complete.

From a clean companion checkout, with the offline dependencies and the
original's native release binaries available, run:

```sh
python3 -B verification/whole/unified-buffered/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision 59f947dafdb4cf3055e35216857472ada895a76a \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

The composition verifier uses Rust 1.97.1 for both compiler and
product. Both receipts pin original
`af54339d250290490d62d4475a77539b7e739071` and companion input
`6a3eb779b60e63bef3dbfbb83e64b8a486ef77df`.

The [macOS ARM64 replay](rust-aarch64-apple-darwin.json) ran the buffered verifier through the Rust workspace qualifier. The downstream verifier checked the projected seven-member lock before the isolated release build, then ran its four buffered-reader tests. Its recorded provider, checksums, cases and outcomes match the original macOS receipt.
