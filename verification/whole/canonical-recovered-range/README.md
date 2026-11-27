# Whole-workspace recovery equivalence for MissingRange

This successor of the [seven-member MissingRange candidate](../canonical-range-data/README.md) selects the [source-recovered range graph and Rust binding](../gap-range-recovery/README.md). The engine continues to select the [source-recovered MissingReason](../gap-reason-recovery/README.md). The recovered standalone package contributes only its range component to the workspace.

The generation stage contains the two recovered graph and binding pairs and the rest of the digest-pinned workspace closure. It excludes the authored reason and range directories and package manifests. The source-denial gate also checks the original model, framework source, baseline engine source, independent caller, and each authored graph and binding.

On each ARM64 host, qualification compares every emitted file and directory path with the retained, already qualified predecessor. This includes generated Rust, Cargo manifests, Cargo.lock, and member package locks. Differences are limited to eight provenance fields across two files.

| File | Changed provenance fields |
| --- | --- |
| `workspace.lock` | Workspace input hash, engine input hash, engine package-lock hash |
| `marmish-engine/package.lock` | Engine manifest hash, range graph and binding hashes, range projection graph and binding hashes |

The verifier also rehashes the complete retained native Cargo library closure and checks that the source-recovered standalone observations match the archived #123 outcomes. All equivalent build input files are matched byte for byte. The #123 native observations provide the scoped behavior evidence for the equivalent generated product.

Use a clean companion checkout, the exact reviewed framework checkout and same-host compiler recorded by the source-recovery receipt, the pinned original checkout, and the retained #123 build directory.

```sh
python3 -B verification/whole/canonical-recovered-range/verify.py INPUT_REVISION \
  FRAMEWORK COMPILER ORIGINAL RETAINED_123_BUILD NEW_WORK_DIRECTORY RECEIPT
```

The [workspace manifest](../../../oddlang/portable/workspace-v7-canonical-recovered-range.json) emits lock `67615fada2203cadc9bd9373e864df7999585849ca57570fa6fda9655c5bd5e3` using recovered-input compiler [9923e666](https://github.com/Epiphytic/oddlang/tree/9923e6664899de66915ed3a94e417f261ffed401). Both hosts staged 105 portable inputs and confirmed eight denied files. Each emitted tree had 45 directories and 101 files. All 84 generated Rust files matched #123 byte for byte. [ARM64 macOS](aarch64-apple-darwin.json) · [ARM64 Linux](aarch64-unknown-linux-gnu.json)

Check the workspace descriptors from the companion repository root:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin workspace-product -- compose-api canonical-recovered-range oddlang/portable --check
```

Omit `--check` to regenerate the descriptors.

The [Rust composer replay on macOS ARM64](rust-aarch64-apple-darwin.json) compared the complete generated tree to the retained authored-range product. It reproduced all 84 Rust files, the eight source denials and the archived native evidence. Only the companion revision differs in the two macOS receipt documents.
