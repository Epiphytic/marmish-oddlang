# Native read-only status owner review

Claude Opus 5.5 approved framework revision `526da0a7533d54bb71bd358dc3e3b26bcefe00bf`, companion generation at `346ca1880a61beb53b3443addec1afb402363c9b`, and the evidence published at `b498ffccbad336d702d72fe6ef1abba0bd561210`.

The framework review identified an unbounded stopping response and a teardown window in which a released listener inode could be reused. Every phase is now checked against the frame bound. The owner removes its pathname once while the listening descriptor remains open. The unrelated release fixture covers oversized stopping policy, explicit and cancelled shutdown, and a replacement socket that survives the original owner's cleanup.

The companion review verified the original package, source and dependency pins, the four scoped admission controls, both independent native ARM64 receipts, and the graph and package lock digests. Both platforms matched the original client on seven frames, activation output, socket mode, exit and cleanup. Provider readiness, installed daemon support and provider shutdown output remain separate.

[Framework PR](https://github.com/Epiphytic/oddlang/pull/46) · [Companion PR](https://github.com/Epiphytic/marmish-oddlang/pull/27) · [Evidence](../../verification/whole/status-owner/README.md)
