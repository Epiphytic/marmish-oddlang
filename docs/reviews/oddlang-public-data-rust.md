# Canonical public data and MDK verifier review

Claude Code 2.1.332 reviewed the canonical public-data composer, standalone MDK verifier, shared MDK witness and their two workspace callers in auto permission mode. It compared both producers to the Python implementation at companion revision `0a1d816` and approved the registry dependency closure, Serde lock change, source mutation, compile diagnostics, Cargo cache locking and receipt fields.

A read-only follow-up confirmed Python-compatible ASCII escaping of ordered JSON, publication of all three MDK reports together, and the staged Cargo lock and source index. The implementer ran the Unicode and ambiguous-registry regression tests, offline Clippy, the four-file composition and three independent drift controls. Original and generated Rust probes matched the archived standalone and six-member evidence on macOS ARM64. [Replay receipt](../../verification/whole/canonical-public-data/rust-qualification-macos-arm64.json)

## Sealed workspace qualifiers

A separate read-only Claude Code 2.1.332 review compared the Rust six-member and canonical seven-member qualifiers with the previous Python implementations. It identified the original vault caller's sandbox visibility, the downstream callback's ability to claim a standalone receipt, missing subprocess deadlines and two diagnostics issues. A follow-up found the core issues resolved. The implementation also preserves archive key ordering and emits an explicitly intermediate downstream receipt until the caller's additional checks pass.

The macOS ARM64 runs used the committed implementation at `8df69cebb1960ca51d95fc2e1eb63860a2488636`. The [six-member receipt](../../verification/whole/unified-mdk/rust-aarch64-apple-darwin.json), [canonical seven-member receipt](../../verification/whole/canonical-public-data/rust-aarch64-apple-darwin.json) and [buffered successor receipt](../../verification/whole/unified-buffered/rust-aarch64-apple-darwin.json) record the native observations. A forged extension requesting the standalone six-member format failed before creating a receipt. Separate controls rejected a changed Cargo lock, a changed sealed engine source file and an extra symbolic link in the native member.

## Buffered workspace and successor callbacks

The read-only review compared the buffered composer, Rust qualifier and downstream callback adapters to companion commit `7e015a3`. The initial review identified an outer timeout that could interrupt restoration of the generated file and diagnostics that omitted native test stdout. The final reader oracle copies the generated workspace into private scratch before adding tests, captures both output streams, and cleans up the callback's process group on timeout. The follow-up source review found no blocking issues. The [controls](../../verification/whole/unified-buffered/rust-callback-controls.json) include a deliberately failing native build, conflicting provider pins and differing predecessor bytes.

## Public API composition

A read-only review compared the three Rust composition paths and their verifier entry points to the previous Python producers. It identified a malformed base manifest that could add a missing prototype member, plus the engine command's path output. The corrected implementation requires the existing member, reports invalid startup descriptors and prints the engine workspace path. Independent Rust and Python replays reproduced six published files byte for byte and rejected fourteen cases, including both malformed structures. [Composition evidence](../../verification/whole/converged-public-api/rust-composition-macos-arm64.json)
