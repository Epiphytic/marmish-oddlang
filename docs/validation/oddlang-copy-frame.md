# Complete copy protocol library on macOS ARM64

The pinned original and source-free generated package passed all 11 unchanged upstream tests and nine extra native tests for canonical frames, public constants and signatures, ordered errors and incremental decoding. The same complete generated package also passed the independent standalone Manifest and Checkpoint corpora. A repinned mutant showed that the exact tests catch changes in Data guard order for encoding and decoding.

See the [reproduction procedure](../../verification/whole/copy-frame/README.md), [host receipt](../../verification/whole/copy-frame/reference-aarch64-apple-darwin.json) and [structural gap inventory](../../verification/whole/copy-frame/structural-gap.md). This qualification covers the public copy protocol library. Installed transport, filesystem effects and authority enforcement need separate runtime evidence.
