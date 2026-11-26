# Native local status client review

Claude Opus 5.5 independently reviewed the generic compiler through `c94b678a9672a75df9fe761dad62cfba9aaff33f` and the companion build at `1449249cfa790992d17e1f951ee9addf577110e0`. Both final reviews approved their scope.

The compiler review found that unknown root argument IDs and child-local shadowing could bypass a guard in a release build. Native package linkage now checks the callback path and the identity, kind and global inheritance of each argument. The unrelated release fixture exercises the rejected links and a real Unix exchange.

The companion review verified the original binary and source pins, the portable Cargo dependency closure, checked manifest and lock digests, sandbox read/write probes, measured parser and guard checks, and the independently observed original daemon exchange. Both native ARM64 receipts match on the contract observations. The Mac receipt separately records three known reply differences and the open paths. Executable hashes identify instances used in each capture.

[Framework PR](https://github.com/Epiphytic/oddlang/pull/42) · [Companion PR](https://github.com/Epiphytic/marmish-oddlang/pull/23) · [Mixed qualification](../../verification/whole/status/client-README.md)
