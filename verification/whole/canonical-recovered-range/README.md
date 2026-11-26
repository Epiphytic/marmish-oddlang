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
