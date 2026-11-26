# Oddlang native CLI parser qualification review

Claude Opus 5.5 independently reviewed the parser projection and companion release qualification. The final framework review covered `c01279eb61ed9e86b51b2eeaf0c5b744397c74a3`. The companion reviewed the isolated build at `c5810d91a83b18bc91cee39efd0123b651aa4904` and approved its follow-up at `31a7658c7bb81491ab05427986eab6eae1d76cbf`.

The initial companion review found accessible Git objects and alternate copies of the original checkout. The runner now stages graph inputs into a Seatbelt file-data allowlist, denies network access, and checks the original source, oracle and both Git object stores are unreadable there. The build pins the clean Oddlang checkout and Rust 1.97.1. The reviewer ran the hardened script and reproduced 34 exact process observations per binary. The final follow-up removed executable hashes from the receipt because build paths alter those bytes. Compatibility evidence uses pinned graph and lock bytes and the independent process transcript.

The framework fixture separately verifies external names with slash separators, inherited option relationships and unsupported enum-level settings. Reverting each fix in a scratch worktree exposed its corresponding test failure. The scope is parser-only on macOS ARM64. Live commands, daemon state, rsync and private helper protocols remain open.

[Companion PR](https://github.com/Epiphytic/marmish-oddlang/pull/17) · [Framework PR](https://github.com/Epiphytic/oddlang/pull/36) · [Native receipt](../../verification/whole/cli/generated-aarch64-apple-darwin.json)
