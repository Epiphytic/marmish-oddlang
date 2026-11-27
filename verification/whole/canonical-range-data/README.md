# Canonical MissingRange data candidate

This separate seven-member workspace appends the verified `marmish_engine::MissingRange` record to the four-type public-data candidate. It uses the exact [standalone graph and Rust overlay](../gap-range-record/README.md). The record binds to the existing source-recovered `MissingReason` identity. Only the engine member gains a projection and re-export. The root and engine Cargo locks and all sibling edges remain the same.

The verifier uses the same sealed native build for the public range import, an external private-module control, all 18 range Serde cases and 17 reason cases, and the public fields and trait fixture. It also reruns the four inherited public-data callers. The observations use release libraries on native ARM64. The callers operate only on values.

From clean checkouts, build with Rust 1.97.1, the [framework source at 7855c055](https://github.com/Epiphytic/oddlang/tree/7855c055dc1e72b49d3654f6e6c500a0ffdb57ec), and the [sealed four-type predecessor product](../canonical-public-data/README.md).

```sh
python3 -B verification/whole/canonical-range-data/verify.py INPUT_REVISION \
  FRAMEWORK ORIGINAL BASELINE_PRODUCT COMPILER_TARGET NEW_WORK_DIRECTORY RECEIPT
```

The [candidate manifest](../../../oddlang/portable/workspace-v7-canonical-range-data.json) emits workspace lock `b3534d6003e1ca3d46484e5119e9f43aad95ed8aa90faad1b60b7a060edb68c0`. Both hosts matched all cases and the public import. Darwin used the [reviewed prebuilt compiler](https://github.com/Epiphytic/oddlang/blob/4e43a3d2367cbb1839ff224ccb24da175c6b8278/verification/public-records/receipts/aarch64-apple-darwin.json) through `--darwin-compiler`. Linux built the compiler from the clean pinned source. [ARM64 macOS](aarch64-apple-darwin.json) · [ARM64 Linux](aarch64-unknown-linux-gnu.json)

Check the workspace descriptors from the companion repository root:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin workspace-product -- compose-api canonical-range-data oddlang/portable --check
```

Omit `--check` to regenerate the descriptors.
