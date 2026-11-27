# Seven-member engine-choice candidate

The [candidate manifest](../../../oddlang/portable/workspace-v7-canonical-engine-choices.json)
replaces the canonical engine member with the qualified
[standalone choice member](../../../oddlang/portable/engine-fieldless-choices-package.json).
It retains the canonical root Cargo manifest and lock, every member
dependency and physical Cargo manifest, and the other six packages'
locked files. The engine keeps its prior components and generated files,
with an added choice projection and append-only exports in the two
Rust facades.

The audit compiles the same 140 frozen import spellings against all
seven candidate libraries. It compares each spelling with the earlier
native audit on the same host. It also reuses the unchanged enum caller
and 17 trait and visibility controls from the standalone qualification
against the actual original and candidate engine libraries.

The two ARM64 replays compile 79 of the 140 spellings, up from 77.
`marmish_engine::Stream` and `marmish_engine::SubmitDisposition`
are the only additions, and every inherited outcome and diagnostic
code and message agrees with the same-host baseline. The engine data
caller matches all four variant, debug and ordinal rows. The trait
checks use the Serde artifact selected from each role's own Cargo build.
[Darwin receipt](aarch64-apple-darwin.json) and [Linux receipt](aarch64-unknown-linux-gnu.json).

## Reproduce

Use native Rust 1.97.1 and cached offline Cargo inputs. Build Oddlang
at `8e3320aacb0106d543c3e21ab1faf60a02132c04` and use the exact native
compiler bytes recorded in the standalone engine choices receipts
([Darwin](../engine-fieldless-choices/aarch64-apple-darwin.json),
[Linux](../engine-fieldless-choices/aarch64-unknown-linux-gnu.json)).
Use the original at `af54339d250290490d62d4475a77539b7e739071` and
companion manifest input revision `934bd7c96af85e0c502cf943b1c1d72f36692a8e`.
Produce the preceding canonical workspace separately with its published
compiler and pins.

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin workspace-product -- compose-api canonical-engine-choices oddlang/portable --check
"$ODDLANG" build native-workspace rust \
  oddlang/portable/workspace-v7-canonical-engine-choices.json "$PRODUCT"
python3 -B verification/whole/canonical-engine-choices/verify.py \
  "$INPUT_REVISION" "$FRAMEWORK" "$ODDLANG" "$ORIGINAL" "$ORIGINAL_TARGET" \
  "$BASELINE_PRODUCT" "$PRODUCT" "$TARGET" "$RECEIPT"
```

Use separate Cargo targets outside both inputs and generated trees. The
candidate workspace lock has SHA-256
`a72d0b0538b3afc272fcd7f5cf8cc900935fd4cc920261fa4bd6bcea46270292`.

## Default promotion

The published default remains the previous canonical workspace. Before
promotion, the candidate needs a same-build replay of the existing
source-isolated workspace gates, local release-binary and CLI cases,
and the complete required unchanged native test baseline. The prior
named suite verified 42 of 909 tests on Darwin ARM64 and 43 of 887
on Linux ARM64, with five ignored cases on each host. This candidate
phase runs compile-only import probes and the local enum data caller.
The promotion evidence remains open.

Omit `--check` from the composition command to regenerate the candidate workspace descriptor.

The [Rust composer replay on macOS ARM64](rust-aarch64-apple-darwin.json) reproduced the candidate member locks and exercised all 140 imports. The seven native libraries exposed 79 imports, including the two new engine choices. The receipt identifies the historical Python composer separately from the Rust checker, its source revision and its executable. Build hashes identify the artifacts used by that run.
