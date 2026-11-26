# Launch hash qualification on macOS ARM64

The [host receipt](../../verification/whole/copy-launch-hash/reference-aarch64-apple-darwin.json) pins the original and generator revisions, Rust toolchain, generated package lock and unchanged native caller. The offline build denied reads from the original and companion repositories. The original, generated library and independent byte oracle agreed across 227 requests. Twenty-six cases checked named validation failures. The original and generated packages also agreed on 45 native constructions and 56 exact decode outcomes. A repinned field-slot mutation yielded both independently predicted changed values.

See the [reproduction command](../../verification/whole/copy-launch-hash/README.md) and [independent review](../reviews/oddlang-copy-launch-hash.md). The receipt covers the library API on this host. Linux and installed copy execution require separate qualification.
