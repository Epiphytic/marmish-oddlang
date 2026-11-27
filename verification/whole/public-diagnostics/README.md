# Public diagnostic event contract

An outside Rust caller invokes the original native library's `diagnostics::{enable, event, stage}` in four fresh processes. The observer captures the caller's final stage, exit status and stderr. Fields, escaping, compact JSON encoding, object-member order, and the terminating newline are checked byte for byte. The context reports the library's version and host OS/architecture.

| Probe | Required observation |
| --- | --- |
| Disabled | An event produces no output; assigning a stage updates the caller's value. |
| Sequence | A pre-enable stage remains silent. Each of two enables emits a context. Quoted text, Unicode, controls and empty values round-trip. A stage assignment emits the new stage with the begin category. |
| Joined threads | An enable completed in one thread is visible to a subsequently started thread. |
| Closed pipe | With no stderr pipe reader, all three functions return; the final stage assignment is visible to the caller. |

Run `cargo +1.97.1 run --locked --offline --manifest-path ../canonical-public-data/Cargo.toml --bin workspace-product -- public-diagnostics /path/to/pinned/original /path/to/cargo-target /tmp/observation.json` from this directory. The verifier builds the original release library offline with the default Cargo features and Rust 1.97.1. It verifies the source revision and Cargo artifact identity before compiling the downstream probe. Each target's JSON receipt records the raw output. The closed-pipe case can observe completion and caller state only. Thread scheduling beyond the explicit joins requires separate evidence.
